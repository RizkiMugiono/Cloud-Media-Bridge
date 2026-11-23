# Media Model Coverage and Runtime Compatibility

**Current implementation:** broad model **discovery and classification** via the Hugging Face Hub, using a data-driven task catalog. **No model-generation, inference runtime deployment or media-artifact retrieval is operational yet.** There is no guarantee that every public model can be run on any GPU.

The canonical, extendable classification catalog is [`config/media-tasks.json`](../config/media-tasks.json). These task names are a mixture of current Hub pipeline tags and anticipated community-extension aliases. A recognized task means **cataloged**, not **executed**.

## Media families in the catalog

| Family | Examples of tasks and media |
|---|---|
| **Image** | Text-to-image, image-to-image, image + prompt editing, segmentation, masks, depth, detection, upscaling families when a matching runtime exists, image understanding |
| **Video** | Text-to-video, image-to-video, image + text-to-video, video-to-video, classification, segmentation, video understanding |
| **Audio** | Text-to-audio, audio-to-audio, sound effects, audio enhancement, audio separation, audio understanding |
| **Speech / voice** | Text-to-speech, ASR/transcription, voice activity detection, diarization, voice conversion, speech translation and voice synthesis |
| **Music** | Music and melody generation, text-to-music, music-to-music, stem/source separation, music-related audio tasks |
| **3D** | Text-to-3D, image-to-3D, point-cloud tasks, mesh/scene families where compatible |
| **Spatial / motion** | Pose, depth/spatial estimation, motion generation, motion transformations and 4D families |
| **Documents** | OCR, document question answering, visual retrieval, document layout understanding |
| **Multimodal** | Image-text-to-text, audio-text-to-text, video-text-to-text, image-text-to-video, any-to-any, visual question answering and mixed media inputs/outputs |

**Music versus audio:** Hugging Face often exposes music and sound-effect generators under the general `text-to-audio` pipeline tag. Model metadata tags like `music` can add a more specific family label. A label is a hint only.

**Media analysis belongs here too.** The project covers non-text media inputs and outputs, including transcription, video understanding, OCR and segmentation, even if the result is text or structured data. Pure text-only LLM hosting remains outside the project scope.

## How discovery works

1. `models_search` queries the live Hub. Optional `query` and `task` arguments can search by model words and/or a specific Hub `pipeline_tag`.
2. `models_inspect` resolves a canonical model URL/ID, immutable revision SHA, access/license metadata and task labels. A model without `pipeline_tag` can be classified using its task tags.
3. Classification checks the known task catalog, then bounded media-related tag hints. It returns `known_media_task`, `inferred_media_task_requires_review`, `unclassified_requires_review`, or `non_media_task_out_of_scope`.
4. An unknown task **is not removed from search results**. It remains discoverable and can be inspected, but its runtime compatibility is unverified.
5. `plan_generation` uses that classification to explain why execution cannot proceed. It never deploys a GPU.

### Read-only commands

```sh
cargo run --locked -- models tasks
cargo run --locked -- models search "music" 10
cargo run --locked -- models search "video" 10
cargo run --locked -- models search-task text-to-audio 10
cargo run --locked -- models search-task image-text-to-video 10
cargo run --locked -- models inspect owner/model
cargo run --locked -- plan owner/model modal
```

MCP equivalents are `media_tasks_list`, `models_search` (e.g. `{"task":"text-to-speech","limit":10}`), `models_inspect`, and `plan_generation`.

Search and inspect contact Hugging Face over HTTPS. No provider credentials, GPU charges or model deployments are involved. For gated/private models, access and authentication may require later user-specific integration; finding a model does not bypass licensing or gated restrictions.

## Separating taxonomy from a working media runtime

| Status | Meaning |
|---|---|
| `known_media_task` | A known pipeline/task tag appears in the local catalog; **not runnable just because it is known** |
| `inferred_media_task_requires_review` | An unfamiliar task appears media-related based on metadata; requires human/runtime inspection |
| `unclassified_requires_review` | Publisher supplied a new or missing task; preserve discovery and request more inspection |
| `non_media_task_out_of_scope` | Known pure text-only task without media evidence; excluded from media inference scope |
| `not_verified` | No model/revision/runtime/GPU execution evidence |

To **actually run** a model, a separate runtime resolver must match architecture/checkpoint format, pinned library/image/recipe, CUDA/driver, task-specific input/output schemas, memory requirements, trust policy, model license, and provider support. The user must approve the exact model and **each deployment request**. Downloaded code and model-card scripts are untrusted and are not executed automatically.

A shared runtime recipe can support many compatible models; genuinely new architectures require new reviewed compatibility recipes. A catalogue entry or `config.json` alone cannot create a working inference service.

## Future extensibility: modalities and artifact transport

Do **not** hardcode application logic to images or video. Runtime and worker contracts should support typed inputs and outputs including:

- `text`, `image`, `video`, `audio`, `speech`, `music`, `3d`, `spatial`, `document`, `embedding`, `timestamps`, and compound/multiple outputs.
- Structured input metadata: prompt, image reference, audio reference, video reference, geometry/scene reference, conditioning input, sampling and model-specific validated settings.
- Validated outputs: image frames, encoded audio, video clips, transcripts, music files, point clouds, meshes, spatial tracks, masks, embeddings and JSON analysis, as actually supported by that runtime.
- Authenticated large asset staging, stream/chunk support where available, checksums, clear retention/cleanup behavior and durable handles for long media jobs.

Two **future worker interface schemas** now specify an open-ended typed-media envelope:

- [Generic media request schema](../schemas/media-request.schema.json) — accepts arbitrary task tags and typed text, image, audio, music, speech, video, spatial or 3D asset references, without a hardcoded image/video-only request shape.
- [Generic media result schema](../schemas/media-result.schema.json) — supports arbitrary artifact kinds with verified byte size, MIME type, SHA-256 and authenticated references.

**These are interface contracts, not implemented RPC endpoints or an inference engine.** A provider's GPU runtime is not universally able to execute all of them. Never fabricate media outputs or claim tests passed without real model/worker evidence.

## References

- [Hugging Face task index](https://huggingface.co/tasks)
- [Hugging Face Hub model search and metadata APIs](https://huggingface.co/docs/huggingface_hub/package_reference/hf_api)
- [Multimodal any-to-any tasks](https://huggingface.co/docs/transformers/tasks/any_to_any)
- [GPU provider architecture](PROVIDERS.md)
