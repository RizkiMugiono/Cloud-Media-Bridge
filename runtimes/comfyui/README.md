# ComfyUI runtime candidates

Source: https://github.com/comfyanonymous/ComfyUI

ComfyUI is a reusable workflow framework for supported image/video generation and transformation. A checkpoint or task tag alone does **not** identify a functioning ComfyUI graph. Runtime selection must require a vetted graph definition, a pinned set of nodes, input/output schema and model SHA.

**OCI route:** audited ComfyUI worker image pinned by digest plus pinned node/workflow revisions.

**Installer route:** a reviewed installation manifest on a pinned GPU base image, with verified node versions and a task-specific worker.

Do not automatically install custom nodes or scripts referenced by arbitrary Hugging Face models. Graphs and external nodes execute code and require security review.

**Current:** Upstream documented as candidate; no approved workflow, image/installer digest or verified worker.
