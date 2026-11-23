# Transformers runtime candidates

Source: https://github.com/huggingface/transformers

Use supported Transformers pipelines for **compatible** speech, audio, document/vision and multimodal models. A `config.json` or `library_name=transformers` is only a loader hint, not proof that the selected model's architecture is supported.

**OCI route:** pinned worker image with validated architecture, preprocessing, model loader and postprocessing.

**Installer route:** a reviewed set of pinned Transformers/PyTorch/CUDA and media dependencies on a pinned base image.

When a model requires `trust_remote_code`, custom attention or operators, stop automatic selection and require independent code review and a vetted runtime recipe. Never automatically execute `auto_map`, `setup.py` or model-card commands. Test exact revision, memory/precision and output artifact types.

**Current:** Source identified; no executable image or installer digest, no verified exact-model GPU test.
