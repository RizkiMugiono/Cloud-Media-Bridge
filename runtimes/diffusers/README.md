# Diffusers runtime candidates

Source: https://github.com/huggingface/diffusers

Use existing Hugging Face Diffusers pipelines for **compatible** image and video generation/editing, identified by a model's pipeline components and documented loader. These are candidates, not guaranteed execution of every `text-to-image`, `image-to-video` or diffusion model.

**OCI route:** a tested inference worker container pinned by an image digest and verified model SHA.

**Installer route:** a reviewed, pinned Diffusers + PyTorch/CUDA environment on a pinned base image, with the same verified worker request/artifact contract.

Before shipping, verify the exact `model_index.json` components, pipeline class, safetensors policy, scheduler and postprocessing, codec dependencies, GPU VRAM and the user's provider integration. Do not invoke model-specific remote code or unsafe checkpoints without separate security review.

**Current:** Source identified; no executable image or installer digest, no verified exact-model GPU test.
