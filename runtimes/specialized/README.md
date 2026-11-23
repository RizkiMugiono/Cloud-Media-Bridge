# Specialized runtime families

For compatible media models that do not use standard Diffusers/Transformers loaders, use a vetted **official upstream runtime** rather than inventing an inference engine. Candidate projects include:

- AudioCraft (music/sound): https://github.com/facebookresearch/audiocraft
- SpeechBrain (speech/voice): https://github.com/speechbrain/speechbrain
- pyannote.audio (diarization/VAD): https://github.com/pyannote/pyannote-audio
- Model-author implementations (3D/motion/custom workflows) only after independent security and compatibility review.

Each individual family requires a pinned loader, verified task schema, compatible GPU image or installer, artifact format and real test evidence for the exact model SHA. **Official project existence is not proof of compatibility with arbitrary model weights.** Arbitrary model-repository code remains untrusted.

No specialized runtime is deployed or executed by the repository today.
