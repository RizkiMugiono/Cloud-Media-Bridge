# Cloud-Media-Bridge — Spesifikasi Repository (Multi-Provider GPU)

**Target repository:** https://github.com/RizkiMugiono/Cloud-Media-Bridge  
**Versi spesifikasi:** 2026-11-23 / arsitektur multi-provider  
**Status:** Rancangan implementasi untuk Codex; BUKAN bukti kode telah dibuat  
**Lisensi repository:** Apache-2.0 (pertahankan `LICENSE` GitHub saat ini)  
**Dokumen berbeda:** [`Cloud-Media-Bridge-TESTING.md`](Cloud-Media-Bridge-TESTING.md) adalah rencana PENGUJIAN, bukan spesifikasi aplikasi.

> **Instruksi Codex:** Baca seluruh dokumen ini DAN `Cloud-Media-Bridge-TESTING.md` sebelum implementasi. Bangun sistem berdasarkan kontrak di sini, gunakan pengujian sebagai release gate. Jangan mengirim commit/push, membuat resource cloud, atau menjalankan job GPU tanpa otorisasi terpisah dari pemilik workspace. Tandai kemampuan belum jadi secara eksplisit, bukan dengan mock yang tampak sukses.

## 0. Tujuan dan batas keputusan yang sudah disetujui

Cloud-Media-Bridge adalah **alat lokal yang diinstal setiap pengguna** untuk menghubungkan AI harness yang dipakainya ke **akun GPU cloud milik pengguna itu sendiri**. Ini bukan SaaS, bukan server publik milik maintainer, bukan marketplace GPU, bukan host LLM, bukan platform pembayaran, dan bukan layanan multi-tenant bersama.

**Keputusan arsitektur final:**

1. **Satu repository, banyak provider** lewat adapter versi jelas. Prioritas implementasi dan **seluruh tes GPU nyata pertama hanya Modal.com**. RunPod Serverless, RunPod Pods, Vast.ai Serverless/Instances, Lambda Cloud GPU VM, dan generic SSH/VM merupakan target adapter berikutnya, bukan fitur yang boleh dianggap sudah bekerja.
2. **Rust untuk local core:** CLI, MCP stdio, model discovery, persetujuan, provider router, jobs, local storage, keamanan, observability, dan adapter API ditulis Rust. Tidak diperlukan server pusat.
3. **Dual inference engine:** Native Rust (Candle, ONNX Runtime, native CUDA/FFI) dan Universal Engine (runtime-family Python/PyTorch/Diffusers/ComfyUI/speech/music/video). Python hanya di runtime universal atau integrasi provider jika memang belum tersedia jalur Rust; tidak menjadikan Python syarat bagi seluruh aplikasi lokal tanpa alasan teknis.
4. **Model dipilih manusia, bukan agent.** Agent boleh mencari, menampilkan, mengusulkan; hanya manusia yang menyetujui **model, task, dan immutable revision SHA**. Engine boleh dipilih otomatis setelah model disetujui; **tidak boleh mengganti model diam-diam**.
5. **Provider/workspace dipilih pengguna.** Memilih model tidak mengizinkan perpindahan provider atau provisioning otomatis. Operasi resource cloud adalah tindakan terpisah yang membutuhkan persetujuan lokal bila membuat/mengubah/menghapus resource.
6. **Semua kategori media, bukan host LLM:** image, video, speech/ASR/TTS, voice, music, SFX/audio, vision, OCR, enhancement, spatial/3D dan kategori masa depan; dukungan runnable per model hanya setelah verifikasi runtime, bukan berdasarkan tag Hugging Face saja.
7. **Eksekusi paralel sesuai sifat model/backend dan kapasitas provider.** Tidak ada batas global satu GPU/job yang dipaksakan repository; batching hanya ketika aman/terverifikasi.
8. **Scale-to-zero untuk serverless** jika provider mendukungnya. Pods/VM/SSH mempunyai kemampuan stop/terminate dan billing persistence yang berbeda; jangan mengklaim semuanya scale-to-zero otomatis.
9. **Repository tidak menetapkan financial cap, usage budget, quota biaya, atau price-based model/GPU gating.** Pembatasan **teknis/keamanan** (ukuran input, VRAM, GPU concurrency, timeout, retries, autentikasi) tetap wajib.
10. **Kiri tidak digunakan**. Gunakan OCI image definitions dan tool build standar/provider jika dibutuhkan; tidak ada runtime container alternatif sebagai kewajiban.
11. **Tanpa shared maintainer infrastructure:** tidak ada akun cloud milik maintainer yang mengeksekusi job pengguna, tidak ada database pengguna bersama atau pengumpulan token/telemetri ke maintainer.

## 1. Arsitektur hubungan antarkomponen

```text
Komputer / lingkungan runtime harness milik pengguna
┌──────────────────────────────────────────────────────────────┐
│ Codex / Claude Code / OpenCode / Hermes Agent / DeepSeek    │
│ harness / Grok-based tool / compatible AI harness lain      │
│             MCP stdio (utama) atau CLI adapter              │
│                            │                                 │
│              Cloud-Media-Bridge (Rust local)                │
│   Discovery → Inspect → Human choice/approval → Plan        │
│   Model Registry → Runtime Matcher → Provider Adapter       │
│   Local SQLite → Job Reconciliation → Artifact Manager      │
└────────────────────────────┬─────────────────────────────────┘
                             │ TLS & account-specific auth
       ┌─────────────────────┼──────────────────────────────────┐
       │                     │                                  │
  Modal Adapter        RunPod/Vast/VM Adapters           Generic SSH Adapter
  (MVP: nyata)         (fase berikut; mock awal)         (fase berikut)
       │                     │                                  │
       ▼                     ▼                                  ▼
  Workspace user       Akun masing-masing                Host milik user
       │                     │                                  │
       └─────────────────────┴──────────────────────────────────┘
                             │
                   Worker Engine A atau B
               (image family yang kompatibel)
                             │
                      Media artifact
                             │
               Local bridge → harness/user

Model sources: Hugging Face Hub pertama. ModelScope/Civitai/lainnya
nantinya memakai Source Adapter, bukan GPU Provider Adapter.
```

**Pisahkan empat identitas:** (1) AI harness client, (2) instalasi bridge lokal, (3) model source account seperti Hugging Face, dan (4) GPU provider account/workspace. Keempatnya tidak otomatis saling log in atau berbagi token. Satu pengguna bisa memiliki beberapa provider profile; setiap job terikat pada tepat satu provider account dan satu environment.

**Control plane lokal bukan berarti tidak ada HTTP worker di cloud.** Engine inferensi bisa membutuhkan private HTTP Server/Function/worker pada akun cloud pengguna, tetapi bukan gateway pusat yang dioperasikan maintainer.

## 2. Dua dimensi pilihan: sumber model vs penyedia komputasi

- **Source Adapter** menemukan *apa* modelnya: Hugging Face dahulu; URL → metadata → immutable SHA → file manifest → task/license/trust requirements.
- **Provider Adapter** menentukan *di mana* GPU disewa/dijalankan: Modal dahulu; provider lain bertahap.
- **Engine Adapter** menentukan *bagaimana* bobot dieksekusi: Native atau Universal family.
- **Harness Adapter** menentukan *bagaimana* agent menggunakan bridge: MCP stdio/CLI; tidak bergantung pada brand LLM.
- **Artifact Transport** memindahkan file input/output, verifikasi checksum, retention dan pemulihan.

Jangan mencampur tiga arti "provider": `model_source`, `compute_provider`, dan `runtime_engine`. API dan database harus memakai nama field berbeda.

## 3. Bentuk platform dan dukungan realistis

| Kelas | Kandidat | Kemampuan karakteristik | Status proyek |
|---|---|---|---|
| Serverless GPU | Modal | Autoscale, worker ke nol, image/Function/Server, volume | **Implementasi & pengujian nyata pertama** |
| Serverless GPU | RunPod Serverless | Endpoint/worker/async job/scale down menurut kebijakan RunPod | Interface + contract mock; implementasi kelak |
| Serverless GPU | Vast.ai Serverless | Endpoint, worker group, model sesuai deployment | Interface + contract mock; implementasi kelak |
| GPU pods | RunPod Pods | Instance lifecycle, persistent disk/volume, worker sendiri | Interface + mock; kelak |
| GPU marketplace | Vast.ai Instances | Offer selection, create/destroy instance, network access | Interface + mock; kelak |
| GPU VM | Lambda Cloud | VM API, instance types, SSH/network & lifecycle | Interface + mock; kelak |
| GPU VM lainnya | Provider dengan HTTP API | Adapter khusus jika kontraknya dapat diimplementasikan | Plugin sesuai provider |
| GPU sewa SSH | Generic SSH/OCI | Remote provisioning terbatas, transfer, agent/worker launch, cleanup | Plugin opt-in; tidak dianggap universal |
| Kubernetes GPU | User-owned cluster | API Kubernetes, pod/job lifecycle, secret/storage | Opsional, fase lanjut |

**Kebijakan kompatibilitas:** jangan menyatakan "semua GPU provider langsung didukung". Klaim benar adalah "arsitektur dapat diperluas ke provider baru". Per-provider feature detection wajib, tanpa stub yang menjawab sukses palsu. Provider/region/GPU availability bisa berubah; query capability pada saat perencanaan.

### Lifecycle berbeda secara fundamental

- `serverless`: dapat secara otomatis menghentikan compute idle jika benar-benar didukung; `scale_to_zero=automatic` hanya jika terverifikasi.
- `pod/vm`: biasanya perlu `stop`, `terminate` atau `delete`; stopping GPU tidak sama dengan menghapus storage/IP dan tidak menjamin biaya nol. Mode `manual_stop`/`terminate_required` harus muncul dalam UI pengguna.
- `ssh/existing_host`: mungkin tidak dapat diprovision atau dimatikan oleh bridge; `lifecycle=externally_managed`.
- Selalu tampilkan **tindakan resource** yang akan dilakukan, provider, akun, resource ID, konsekuensi lifecycle; butuh consent tindakan tersebut jika membuat/mengubah/menghapus resource. Ini bukan financial approval/budget cap.

## 4. Provider adapter contract — inti abstraksi

Rust trait berikut adalah **interface konseptual**; Codex harus membuat tipe `async` nyata, serialisasi dan errors sesuai pustaka yang dipilih.

```rust
#[async_trait::async_trait]
pub trait ComputeProvider: Send + Sync {
    fn id(&self) -> ComputeProviderId;
    async fn authenticate_status(&self, profile: &ProviderProfile) -> Result<Identity>;
    async fn capabilities(&self, profile: &ProviderProfile) -> Result<ProviderCapabilities>;
    async fn list_resources(&self, scope: &ProviderScope) -> Result<Vec<Resource>>;
    async fn plan(&self, req: &ExecutionPlanRequest) -> Result<ExecutionPlan>;
    async fn provision(&self, approved: &ApprovedProvision) -> Result<Deployment>;
    async fn submit(&self, job: &ApprovedJob) -> Result<RemoteJobHandle>;
    async fn reconcile(&self, handle: &RemoteJobHandle) -> Result<RemoteJobSnapshot>;
    async fn cancel(&self, handle: &RemoteJobHandle) -> Result<CancelOutcome>;
    async fn list_artifacts(&self, handle: &RemoteJobHandle) -> Result<Vec<RemoteArtifact>>;
    async fn cleanup(&self, approved: &ApprovedCleanup) -> Result<CleanupReport>;
}
```

**ProviderCapabilities** (nilai tri-state `supported`, `unsupported`, `unknown`, disertai evidence/timestamp):

- `provision_method`: `serverless_endpoint|function|pod|vm|external_host`; `supports_gpu`, `supports_cpu`, GPU types, regions, compatible container/image methods.
- `scale_to_zero`: `automatic|manual_stop|terminate_required|externally_managed|unavailable|unknown`; idle policy dan cold-start semantics.
- `job_modes`: sync/async/stream/batch, max request duration (jika ada), cancellation semantics, state query/reconciliation, retry/idempotency support.
- `network`: HTTPS endpoint auth, private endpoint availability, ingress policy, max payload/response, transfer method.
- `artifact_storage`: local staging, attached volume/object store, storage lifecycle, expiration, atomic write and concurrency semantics.
- `runtime`: container image compatibility (OCI), CUDA/driver requirements, GPU memory/compute data, multi-GPU availability.
- `scale`: per-worker safe concurrency, replicas, queue behavior, quota/capacity reporting (bukan biaya).

**Invariant adapter wajib:**

1. Semua method terikat ke `(provider_id, account_identity, workspace_id, environment_id)` yang benar; tidak boleh memakai credentials dari profile lain.
2. Semua operasi yang mengubah resource wajib memvalidasi persetujuan provisioning/cleanup dari UI lokal, terpisah dari model approval.
3. `plan()` bersifat read-only; tidak menyalakan instance/submit job sebagai efek samping.
4. `submit()` wajib membawa idempotency/correlation ID dan menyimpan handle remote jika diterima.
5. Operasi `unknown` tidak diubah diam-diam menjadi `supported` atau diasumsikan sukses.
6. `cancel` hanya menyatakan `confirmed` jika provider menyatakan demikian; dukung `requested|confirmed|uncertain|unsupported`.
7. Retry hanya untuk operasi yang aman/idempotent atau setelah reconciliation; jangan menggandakan job jika respons awal hilang.
8. Semua resource memakai namespace berbasis **instalasi pengguna + provider profile + runtime version**; jangan pernah menghapus resource yang tidak dapat dibuktikan milik bridge.
9. Normalisasi errors tanpa kehilangan `provider error code`, request ID, dan konteks aman. Token harus disunting.
10. Jaminan cleanup berbeda per provider; tampilkan resource tertinggal jika API tidak dapat memastikan shutdown.

### Implementasi Modal pertama

- Gunakan resmi Modal CLI/API yang benar-benar tersedia untuk bootstrap dan deployment dalam workspace pengguna. **Jangan mengarang API Rust native**. Modal CLI berbasis Python dapat dijadikan dependency *untuk provider Modal* jika diperlukan; control plane bridge tetap Rust.
- Identity/workspace/environment diverifikasi saat setup dan sebelum setiap operasi yang memodifikasi resource atau memanggil job; jangan diam-diam beralih akun.
- Pilihan awal deploy via **Modal Server dari OCI image** untuk synchronous worker; autentikasi proxy token dan 503 cold-start retry berdasarkan docs resmi.
- Untuk durable asynchronous video/music, evaluasi Modal Functions `spawn` dan `FunctionCall` (jalur integrasi CLI/SDK yang benar-benar diuji), bukan thread detached pada HTTP worker. Jika user-side control binding memerlukan helper Python berukuran kecil, tampilkan sebagai integrasi provider khusus, dokumentasikan, dan tetap jangan buat gateway bersama.
- `min_containers=0`, `buffer_containers=0` bila resource Modal yang dipakai mendukung parameter. Startup timeout, exit grace, max/target concurrency harus profile-specific dan teruji; jangan menyetel satu nilai untuk semua model.
- Persist bobot di Modal Volume, tetapi hindari concurrent modification file yang sama. Remote job result/queue tidak boleh diperlakukan sebagai database permanen.
- Modal hanya pengujian live dalam rilis pertama. `provider=modal` runnable; adapter lain jelas `planned` sampai credentialed integration test masing-masing dilakukan di fase masa depan.

## 5. Harness compatibility (multi-harness, bukan hanya Codex)

**Codex adalah pembangun sekaligus salah satu klien.** Client lainnya: Claude Code, OpenCode, Hermes Agent, DeepSeek-based harness, Grok-based wrapper, dan MCP-capable agent. Sebutan DeepSeek atau Grok sebagai **model AI** tidak sama dengan keberadaan harness MCP tertentu. Hindari janji kompatibilitas tanpa mengidentifikasi produk dan versi clientnya.

- Default **MCP stdio**: binary lokal diluncurkan oleh harness yang berada di lingkungan eksekusi sama; proses remote-only tidak dapat otomatis menjalankan binary di laptop pengguna.
- `stdout` khusus JSON-RPC MCP; log, status, ANSI, prompt, stacktrace ke `stderr` (redacted) atau file lokal.
- CLI mandiri berfungsi meskipun harness tidak mendukung MCP; adapter dapat memanggil CLI dengan user consent.
- `harness configure` hanya mengubah konfigurasi setelah konfirmasi pengguna, membuat backup, menulis atomik, memelihara setting lain, mendukung rollback.
- Unit test protocol negotiation, `tools/list`, call, invalid schema, reconnect, cancellation dan versi klien.
- Matrix status per client: `tested/version/platform`, `documented-unverified`, `unsupported`; jangan menulis nama agent di README sebagai verified hanya karena mendukung MCP secara teoritis.

**MCP read-only tools:** `list_providers`, `provider_capabilities`, `models_search`, `models_inspect`, `models_compare`, `runtimes_list`, `jobs_status`, `jobs_list`, `artifacts_list`, `doctor`.

**MCP stateful tools (tidak bisa menyetujui sendiri):** `request_model_approval` (membuat `PENDING`), `prepare_deployment` (hanya plan), `submit_generation` (butuh grant), `jobs_cancel`, `artifacts_fetch`. Tidak ada tool MCP `approve=true` atau `switch-provider` terselubung.

## 6. Model discovery, status kompatibilitas, lisensi

### Model source abstraction

`ModelSource` dengan operasi `resolve_url`, `get_metadata`, `list_files`, `resolve_revision`, `download_verified`. Hugging Face didukung pertama; adapter ModelScope/Civitai/lainnya dapat ditambahkan tanpa mengubah `ComputeProvider`.

**Alur URL:**

1. Canonical URL (HTTPS only, host allowlist, reject credentials in URLs, loopback/private addresses, SSRF and redirects tidak aman).
2. Ambil metadata, task/mode, publisher, model card, lisensi, arsitektur/config, dependencies, size, gated/auth flags, daftar file.
3. Resolve branch/tag ke **immutable revision SHA** sebelum pilihan pengguna.
4. Detect format bobot: prefer `safetensors` bila kompatibel; jangan langsung load pickle atau `trust_remote_code=True`.
5. Cocokkan checkpoint family dan exact adapter manifest; periksa task, input/output, architecture, scheduler/decoder, memory, platform support.
6. Bedakan `discovered`, `candidate-compatible`, `verified`, `unsupported`, `restricted`, `unknown-risk`. `verified` memerlukan bukti inferensi nyata untuk `(model SHA, runtime digest, GPU profile, task, tested parameters)`.
7. Tampilkan model yang bisa dipilih; **user memilih sendiri**. Agent tidak boleh memilih otomatis meski ada urutan rekomendasi.

Jangan menyamakan lisensi repository Apache-2.0 dengan lisensi model; gated model dan ketentuan penggunaan model berlaku terpisah.

### Model catalogue dan pembaruan

Tidak ada daftar model ID statis wajib. Catalogue secara dinamis mengikuti provider API; cache metadata boleh memiliki TTL tetapi snapshot SHA job immutable. Auto-update metadata boleh dilakukan; **jangan otomatis menjalankan revisi baru** karena approval lama tidak berlaku untuk SHA baru. Model baru butuh engine support bila arsitekturnya baru.

## 7. Persetujuan pengguna — state machine tegas

**Dua jenis persetujuan berbeda:**

- **Model approval**: `provider_profile, model_source, canonical_model_id, revision_sha, task, trust_profile, parameter_constraints, scope(one-shot/reusable), expiration`.
- **Resource approval**: `compute_provider, account/workspace/environment, operation(provision/update/stop/delete), exact resource parameters, idempotency_nonce`; tidak dapat digantikan oleh model approval.

**State:** `DRAFT → PENDING → APPROVED | DENIED | EXPIRED | REVOKED`. `APPROVED` hanya dari antarmuka manusia terpercaya (TTY independen/OS-native UI, bukan output teks agent atau parameter MCP). Ketika perlu, user menjalankan `cloud-media-bridge approve --request-id ...` di terminal yang dikontrolnya; program harus mendeteksi TTY sebenarnya dan menolak mode headless. Pastikan stdio MCP tidak dirusak prompt TTY.

**Keterbatasan keamanan yang wajib dijelaskan:** jika suatu agent diberi izin shell penuh dalam account OS yang sama, ia mungkin mampu mengubah SQLite atau menjalankan binary approval; tidak ada jaminan approval lokal aman melawan malware/shell agent dengan hak penuh. Dokumentasikan mitigasi OS permissions, protected keychain, separate privilege boundary bila tersedia, serta batas kepercayaan yang sebenarnya. Jangan mengklaim anti-bypass absolut.

**Wajib gagal tertutup:** model SHA berubah, task berubah, trust policy berubah, profile/provider/workspace berubah, grant expired/revoked, atau engine fallback mencoba model berbeda ⇒ tolak sebelum pengiriman GPU. Engine berubah untuk model SHA sama hanya jika adapter kompatibilitas terverifikasi dan trust level tetap memenuhi grant.

## 8. Dual engine dan routing model-aware

### Engine A — Rust Native

Candle, ONNX Runtime, native CUDA/C++/FFI. Tidak semua model Hugging Face dapat dijalankan. Gunakan actual backend evidence, format operator/cuda support dan per-family runtime manifests. Native bukan sinonim latency terbaik.

### Engine B — Universal Runtime

Python/PyTorch, Diffusers, Transformers, ComfyUI workflow runtimes, speech, voice, video, music, custom-vetted; **image terpisah per family** untuk mengurangi container size/cold start. Menggunakan Python dibenarkan karena kompatibilitas model; Rust tetap melakukan seluruh local orchestration.

**Default trust:** jangan menjalankan arbitrary remote Python code, untrusted checkpoint pickle, postinstall script, atau `trust_remote_code=True` secara otomatis. Jika isolasi custom code tidak dapat dibuktikan dan diberi izin eksplisit berdasarkan ancaman yang jelas, kembalikan `MODEL_UNSUPPORTED_UNSAFE`.

### RuntimeManifest (versioned, machine-readable)

```yaml
runtime_id: universal.diffusers.sdxl
engine: universal
manifest_version: 1
image_digest: sha256:<image-digest>
tasks: [text-to-image, image-to-image]
architectures: [sdxl]
weight_formats: [safetensors]
input_schema_ref: schemas/tasks/text-to-image.json
output_schema_ref: schemas/media-output.json
gpu_requirements: {min_vram_gb: 16, cuda_capability: null}
concurrency: {max_safe_inflight_per_worker: 1, batching: false}
execution: {sync: true, async: true, cancellation: best-effort}
trust_policy: {remote_code: denied, pickle: denied}
verification: [] # populate only with signed/traceable real tests
```

Nilai di atas adalah **contoh format, bukan fakta SDXL atau janji async runtime**; profil aktual harus diukur. Manifest harus menyediakan runtime digest/lockfiles, engine version, input/output contract, memory headroom, checkpoint family match dan referensi pengujian.

### Router

1. Validasi human model approval dan compute provider profile.
2. Cari runtime family yang menyatakan mendukung exact model architecture/task/format.
3. Hilangkan runtime yang tidak compatible, trust risk melampaui izin, atau GPU yang tidak memenuhi kebutuhan.
4. Pilih engine tercepat **berdasarkan bukti benchmark yang sebanding**; kalau belum ada evidence, fallback ke prioritas deterministik terdokumentasi, bukan tebakan.
5. Jangan berpindah ke model lain tanpa consent baru. Provider switching juga memerlukan pemilihan provider/workspace baru oleh pengguna.
6. Jika worker gagal, retry sesuai idempotency; fallback ke engine lain hanya jika model SHA sama, plan tervalidasi dan tidak menggandakan job.
7. Profil concurrency adalah per model/runtime/GPU/VRAM: satu worker dapat melayani lebih dari satu request **hanya bila aman**; autoscaler dapat menambah worker/replica menurut provider limits. Tidak ada batas keuangan dalam router.

## 9. Job lifecycle, paralelisme, recovery dan cancellation

**Job state machine:** `CREATED → APPROVAL_REQUIRED | READY → SUBMITTING → QUEUED → PROVISIONING → RUNNING → FINALIZING → COMPLETED`, cabang `FAILED | CANCEL_REQUESTED → CANCELLED | CANCEL_UNCERTAIN | EXPIRED | ORPHANED`. Semua state transition dicatat atomik dan idempotent; jangan menandai COMPLETED sebelum artifact tervalidasi.

- **Synchronous short-job:** request hasil langsung hanya jika deadline dan runtime mendukung. Modal Server 503 cold start ditangani bounded retry dengan exponential backoff + jitter. Deadline harus diketahui; error terklasifikasi.
- **Durable async:** pekerjaan video/music berumur panjang memerlukan handle remote yang dapat diambil kembali setelah proses bridge/harness mati. Prove `submit → store remote ID → restart → poll → fetch`; detached local thread tidak cukup.
- **Ambiguous submission:** bila jaringan putus setelah request terkirim, lakukan query/reconciliation berdasar idempotency_key; **jangan mengirim ulang membabi buta** jika backend tidak menjamin dedupe.
- **Parallel execution:** worker berbeda boleh eksekusi concurrent; worker sama mengikuti `max_safe_inflight`, VRAM headroom, cache coherence dan batch rules. Queue/backpressure teknis mencegah VRAM/OOM storm, bukan quota keuangan.
- **Cancellation:** status `requested/confirmed/uncertain`; jika cancellable tidak tersedia, beri informasi; cleanup resource hanya setelah state aman.
- **Resource lifecycle:** serverless idle→0 jika supported; VM/pod dapat memerlukan stop/terminate manual/provision policy, dan pengguna harus mengetahui resource yang masih aktif.
- **Journal local:** SQLite WAL dan schema migrations; local state membantu recovery, bukan source of truth remote. Simpan remote resource IDs, status, artifacts, cleanup status, origin profile, model SHA.

### Normalized JobRequest dan RemoteJobHandle

```json
{
  "schema_version": 1,
  "job_id": "generated-uuid",
  "idempotency_key": "generated-uuid",
  "compute_provider": "modal",
  "provider_profile": "personal-modal",
  "account_id": "provider-resolved-identity",
  "workspace_id": "provider-resolved-workspace",
  "model_source": "huggingface",
  "model_id": "owner/model",
  "revision_sha": "immutable-commit-sha",
  "task": "text-to-image",
  "model_approval_ref": "grant-id",
  "engine": "native-or-universal",
  "runtime_manifest_digest": "sha256:<digest>",
  "inputs": {"prompt": "An illuminated forest"},
  "input_asset_refs": []
}
```

`RemoteJobHandle` additionally includes opaque provider job ID, provider resource ID, transport mode, submission time, expiration/retention if known, last reconciled remote state, retries/attempts; do not expose secrets in serialized records.

## 10. Input assets, model weights, cache dan artifacts

### Input/Output

- Format `InputAsset`: local path/ref, media MIME *validated from bytes*, byte size, SHA256, duration/dimensions if known, uploader identity locally, staging ref, TTL, upload progress. Support large inputs via chunked/resumable transfer **only when provider transport supports it**.
- Never send large video bytes embedded in JSON MCP. Use file reference or staged upload with authentication; no public world-readable source URL default.
- Validate decode bombs, archive traversal, huge frames/durations, unsafe codecs, malformed metadata, media MIME mismatch.
- Output `Artifact`: job UUID, media type, size, SHA256, provider storage ref, created/expires, verified local path, download state. Download via authenticated transport, hash verify before `COMPLETED`.
- Model weights cached by `(source, model_id, revision_sha, file digest, runtime family)` with immutable committed file/path; incomplete downloads write temp paths and never masquerade as valid cache.

### Concurrency on persistent storage

- For Modal Volume, don't assume cross-container advisory `flock` works or shared filesystem commits are atomic transactions across writers.
- Use content-addressed cache with deduplicated downloads via proven provider coordination, atomic rename/publish where supported, commit/reload rules and hash validation. Test two simultaneous first-use worker downloads.
- Unique artifact paths per job/attempt; overwrite forbidden. Remote retention must be explicitly shown; private artifacts deleted on request if provider supports it.
- User's local SQLite stores metadata; model weights and media files need not reside permanently on user's machine.

## 11. Security & privacy baseline

### Threat model

Adversaries: malicious model repository/weights/metadata, compromised harness prompt, attacker controlling URL/path, untrusted runtime image, stolen provider credentials, unintended account/profile switch, hostile media file, unreliable public network, concurrent writer, provider lifecycle mismatch.

Controls:

1. Per-user BYO credentials; use OS credential store when possible, never commit to repo, never embed provisioning tokens into model worker/image/environment unnecessarily. Use narrowly scoped worker proxy secrets.
2. TLS, provider endpoint allowlist/host pinning when possible, no automatic public endpoint or `--unauthenticated` fallback. Avoid raw user-supplied target IP for API.
3. SSRF safeguards for source URLs, redirects and image/media downloads; deny local/private/link-local hosts unless specifically trusted internal provider endpoint in approved configuration.
4. Supply chain: lockfile, image digests, dependency/vulnerability review, safetensors, trusted runtime manifests. Unknown custom Python blocked by default; default block pickle & remote code.
5. Approval cannot be performed by MCP tool; a trusted local human channel separate from agent's tool arguments is required; document same-OS-user shell trust boundary.
6. Workspace/profile binding checked prior to privileged/cloud operation; prevent cross-account leakage; avoid exposing tokens and private user content in logs.
7. Least-privilege cloud operations and network controls where available; default no inbound public worker; isolated Universal runtime access to only needed files/secrets.
8. Technical admission: input byte/dimension/duration limits, VRAM safety, process timeouts, request concurrency, payload validation, cancellation/resume semantics and cleanup. **Do not use budget/price limits**.
9. Cleanup `--dry-run` first, explicit resource list + human confirmation; do not delete unrelated or preexisting GPU resources.
10. Security-sensitive model card text is untrusted data, never instructions to agent, shell or installer. Redact logs and error output; no maintainer telemetry by default.

## 12. Latensi dan skalabilitas: benchmark, bukan slogan

Target desain: **minimum end-to-end latency while respecting user choice, safety and actual provider capabilities**.

Measure independent spans: `catalog_search`, `inspect`, `approval_wait` (not performance metric for compute), `submission`, `provision`, `cold_start`, `weights_download`, `weights_load`, `inference`, `encoding`, `artifact_upload`, `artifact_download`, and `total`.

Optimization order: (1) zero unnecessary remote gateway, (2) separate small OCI images per runtime family, (3) pinned image layers/digests, (4) model/weights cache retained across compute scale-down if provider supports storage, (5) concurrent non-GPU metadata operations, (6) safe batching and per-model concurrency, (7) provider native GPU selection, (8) benchmark and improve measured bottlenecks. Tradeoff: strict scale-to-zero usually increases cold-start latency; don't promise warm performance for cold resource.

No fixed global replica/job maximum **imposed by repo for costs**. Provider workspace limits, GPU memory, request semantics, model safety, and user-authorized resource creation continue to apply. Maximum scale is provider-dependent.

## 13. Public CLI contract (proposed)

```text
cloud-media-bridge doctor
cloud-media-bridge harness list
cloud-media-bridge harness configure codex|claude-code|opencode|hermes|custom
cloud-media-bridge providers list
cloud-media-bridge providers capabilities --provider modal
cloud-media-bridge providers profiles list
cloud-media-bridge providers setup modal
cloud-media-bridge providers use modal --profile PERSONAL
cloud-media-bridge resources plan --runtime universal.diffusers
cloud-media-bridge resources deploy --plan-id <ID>       # interactive approval
cloud-media-bridge resources list
cloud-media-bridge resources cleanup --dry-run
cloud-media-bridge models search --source huggingface --task text-to-image
cloud-media-bridge models inspect <URL>
cloud-media-bridge approvals request --model <URL> --task text-to-image
cloud-media-bridge approvals approve --request-id <ID>  # human TTY only
cloud-media-bridge approvals list|revoke
cloud-media-bridge generate --model <URL> --task <TASK> --inputs <JSON>
cloud-media-bridge jobs list|status|cancel|fetch <JOB_ID>
cloud-media-bridge artifacts verify|fetch|delete <ARTIFACT_ID>
```

Missing adapter yields `PROVIDER_NOT_IMPLEMENTED`/`FEATURE_UNSUPPORTED`; it must not attempt to send a request to Modal as an unapproved fallback. CLI options/providers are versioned; future commands may change only with docs and tests.

## 14. Repository structure (files for actual product)

```text
Cloud-Media-Bridge/
├── Cargo.toml; Cargo.lock; rust-toolchain.toml
├── README.md; LICENSE; .gitignore; .env.example
├── AGENTS.md
├── Cloud-Media-Bridge.md                 # INI: implementasi repository
├── Cloud-Media-Bridge-TESTING.md         # HANYA: test plan Codex
├── crates/
│   ├── bridge-cli/                       # local Rust CLI
│   ├── bridge-mcp-stdio/                 # tools/transport MCP
│   ├── bridge-harness/                   # per-client config adapters
│   ├── bridge-sources/                   # HF resolver + future sources
│   ├── bridge-model-inspector/           # metadata, SHA, license, trust
│   ├── bridge-approval/                  # human approval independent UI
│   ├── bridge-router/                    # model/runtime/provider planning
│   ├── bridge-provider-api/              # trait, capabilities, errors
│   ├── bridge-provider-modal/            # working first provider
│   ├── bridge-provider-runpod/           # skeleton/contract mocks only
│   ├── bridge-provider-vast/             # skeleton/contract mocks only
│   ├── bridge-provider-lambda/           # skeleton/contract mocks only
│   ├── bridge-provider-ssh/              # skeleton/contract mocks only
│   ├── bridge-runtime-registry/          # manifests/verification matrix
│   ├── bridge-jobs/                      # journal/reconcile/async
│   ├── bridge-assets/                    # staging/uploads/downloads
│   ├── bridge-local-store/               # SQLite migrations
│   ├── bridge-security/                  # URL validation, redaction
│   ├── bridge-types/                     # shared typed JSON contracts
│   └── bridge-telemetry/                 # local logs/benchmarks only
├── engines/
│   ├── native/{candle,onnx,cuda-ffi}/
│   └── universal/{diffusers,transformers,comfyui,speech,voice,music,video,custom-vetted}/
├── providers/
│   ├── modal/{deployment,profiles,capabilities}/
│   ├── runpod/{serverless,pods}/          # plans, no live claims
│   ├── vast/{serverless,instances}/       # plans, no live claims
│   ├── lambda/{instances}/               # plans, no live claims
│   └── generic-ssh/                      # plans, no live claims
├── runtimes/manifests/                  # pinned image + capability metadata
├── deployment/{modal,images,profiles}/   # no CI auto deployment
├── schemas/{provider,model,approval,job,artifact,runtime}/
├── migrations/                           # SQLite
├── docs/{ARCHITECTURE,QUICKSTART,PROVIDERS,HARNESSES,DUAL_ENGINE,SECURITY,PRIVACY,SCALE_TO_ZERO,LIMITATIONS}.md
├── examples/{harness-configs,model-requests}/
├── tests/{unit,contracts,security,integration,fixtures,benchmarks}/
└── .github/workflows/                   # offline CI; live jobs manual opt-in
```

README must distinguish **implemented / verified / planned** features using dated compatibility matrix. No fake icons or green checkmarks for untested providers/models. `AGENTS.md` tells all coding agents to consult the two specs and not deploy automatically.

### .gitignore additions (mandatory)

```gitignore
.env
.env.*
!.env.example
*.pem
*.key
*.p12
*.sqlite
*.sqlite-*
*.db
secrets/
credentials/
artifacts/
outputs/
cache/
.model-cache/
__pycache__/
.venv/
venv/
.pytest_cache/
.pyre/
*.log
!docs/examples/*.log.example
```

Do not accidentally ignore legitimate Rust source, lockfiles or tests. Never commit actual provider tokens, HF gated tokens, binary output, model weights, or generated media. Use GitHub secret scanning and review CI logs.

## 15. Implementation phases + release gates

**Phase 0 — Contracts & threat model (offline):** shared types, adapter trait, capabilities, error taxonomy, SQLite schema, approval model, provider/account binding, versioned worker protocol. All tests offline; no provisioning.

**Phase 1 — Local Rust bridge (offline):** MCP stdio + CLI, Hugging Face discovery/resolve SHA, read-only inspect, harness config, human approval UI, model/runtime registry, local persistence, strict errors. Validate security tests including approval bypass before cloud operations.

**Phase 2 — Modal account integration:** official Modal CLI discovery, workspace identity, plan/provision/cleanup with separate confirmation, authenticated private worker, OCI image digest, scale-to-zero. Real tests only after explicit user authorization.

**Phase 3 — Two real engine implementations on Modal:** one Native image generation model and one Universal image generation model at pinned SHA, real artifact hash, cold/warm timings, reproducible evidence.

**Phase 4 — Async, parallel, assets:** video/audio transfer contracts; verified remote durable job semantics; restart recovery; concurrent models/workers; content-addressed cache and cancellation. No claim of video/music support until real models run.

**Phase 5 — Multi-harness compatibility:** verify Codex, Claude Code, OpenCode, Hermes where available; record exact versions/platform; DeepSeek/Grok wrappers as individually tested only; generic MCP conformance.

**Phase 6 — Additional GPU providers (after Modal green):** implement one provider at a time with its own real integration suite and lifecycle/docs. Start RunPod Serverless/Pods, Vast.ai, Lambda, generic SSH by demand. Do not imply live tests were run for them.

**Production release requires:** actual Rust bridge, Model SHA approval path, no cross-profile operation, only authenticated worker, verified real media inference, scale-to-zero evidence for Modal, tested retry/reconcile, safe parallel cache/assets, no silent model/provider switch, no repo maintainer backend, and completed test plan evidence. Unsupported feature labels remain truthful.

## 16. Machine-readable errors (examples)

`HARNESS_ENV_UNSUPPORTED`, `MCP_PROTOCOL_UNSUPPORTED`, `PROVIDER_NOT_IMPLEMENTED`, `PROVIDER_CAPABILITY_UNKNOWN`, `PROVIDER_AUTH_REQUIRED`, `PROVIDER_PROFILE_MISMATCH`, `PROVIDER_RESOURCE_APPROVAL_REQUIRED`, `PROVIDER_LIFECYCLE_UNSUPPORTED`, `MODEL_APPROVAL_REQUIRED`, `MODEL_REVISION_CHANGED`, `MODEL_UNSUPPORTED`, `MODEL_UNSUPPORTED_UNSAFE`, `MODEL_ACCESS_RESTRICTED`, `RUNTIME_UNVERIFIED`, `INPUT_INVALID`, `ASSET_TRANSFER_FAILED`, `WORKER_UNAVAILABLE`, `REMOTE_SUBMISSION_UNCERTAIN`, `JOB_STATE_UNAVAILABLE`, `JOB_CANCEL_UNCERTAIN`, `ARTIFACT_EXPIRED`, `ARTIFACT_HASH_MISMATCH`.

Response includes typed code, retryability, safe action user can take, correlation ID and provider error code (if relevant). No secret payload or private URL token in diagnostics.

## 17. Explicit exclusions dan anti-patterns

- **Tidak** menaruh semua model ID di source code atau mengklaim seluruh Hugging Face langsung runnable.
- **Tidak** deploy ke workspace maintainer, database user pusat, API komersial, LLM hosting atau web UI publik.
- **Tidak** membiarkan agent melakukan self-approval model atau resource creation.
- **Tidak** membiarkan router berpindah provider/model tanpa user choice.
- **Tidak** menerapkan pembatas biaya, tetapi tetap menggunakan batas teknis yang diperlukan untuk correctness.
- **Tidak** membangun satu universal image berisi semua dependensi.
- **Tidak** menganggap `stop` pada GPU VM = zero cost, atau local background thread = durable async.
- **Tidak** mengasumsikan Rust native API tersedia untuk semua cloud provider.
- **Tidak** menggunakan Kiri.
- **Tidak** menjalankan arbitrary Hugging Face Python dan pickle tanpa threat assessment + trust permissions.
- **Tidak** memalsukan status provider sebagai implemented/verified, termasuk saat hanya ada skeleton.

## 18. Referensi dokumentasi resmi (periksa saat implementasi)

- Modal Servers: https://modal.com/docs/cli/latest/server
- Modal server scale-to-zero dan 503: https://modal.com/docs/examples/server
- Modal Function async `spawn`: https://modal.com/docs/sdk/py/latest/Function
- Modal Volume concurrency: https://modal.com/docs/sdk/py/latest/Volume
- RunPod API: https://github.com/runpod/docs/blob/main/api-reference/overview.mdx
- RunPod Serverless: https://github.com/runpod/docs/blob/main/serverless/endpoints/overview.mdx
- Vast.ai API: https://vast.ai/developers/api
- Lambda Cloud API: https://docs.lambda.com/api/cloud
- MCP stdio: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/basic/transports/stdio.mdx
- Hugging Face model serialization risk: https://huggingface.co/docs/hub/security-pickle

**Catatan referensi:** API provider berubah. Codex wajib memverifikasi perintah, flags, default auth, retensi dan lifecycle terhadap docs terbaru; dokumentasikan versi dan tanggal pengujian. Tautan adalah referensi desain, bukan bukti implementasi.

## 19. Work order untuk Codex

1. Gunakan repo sebenarnya `RizkiMugiono/Cloud-Media-Bridge` dan pertahankan Apache-2.0.
2. Baca `Cloud-Media-Bridge.md` (produk) dan `Cloud-Media-Bridge-TESTING.md` (pengujian); buat implementation checklist dengan traceability ke test IDs.
3. Bangun komponen Rust/core sesuai fase, mock provider non-Modal secara jujur, tambah satu adapter Modal yang benar-benar dapat diuji.
4. Prioritaskan keamanan profile/approval, model revision, schema, async reconciliation sebelum menambah banyak runtime media.
5. Semua operasi cloud nyata membutuhkan otorisasi pengguna terpisah; test CI offline default. Jangan commit/push/menjalankan GPU tanpa persetujuan eksplisit.
6. Berikan hasil source code, test reports, reproducible steps, error matrix, keterbatasan nyata, dan bukti inferensi asli ketika live test disetujui.
