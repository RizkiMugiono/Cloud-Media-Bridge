# Cloud-Media-Bridge — Rencana Pengujian Repository untuk Codex

**Target:** https://github.com/RizkiMugiono/Cloud-Media-Bridge  
**Pendamping:** [`Cloud-Media-Bridge.md`](Cloud-Media-Bridge.md) = spesifikasi repository/implementasi.  
**Dokumen ini** = prosedur pengujian, evidence, pass/fail, dan release gates. **Jangan menyamakan test mocks dengan kemampuan provider nyata.**

> **Kontrak safety:** Tidak membuat Modal App, Server, Function, Volume, mengakses GPU, menghentikan instance, mengubah konfigurasi harness, mengedit workspace pribadi atau menulis GitHub sebelum perizinan eksplisit. `cargo test` dan lint selalu offline/mocked secara default. Provider selain Modal tidak boleh dikenai live tests pada fase saat ini.

## 0. Tujuan dan tingkat pengujian

| Tier | Jenis | Cloud nyata? | Kapan wajib |
|---|---|---|---|
| T0 | Rust compile/lint/unit/schema/property/fuzz | Tidak | Semua PR |
| T1 | MCP/harness, approval, provider contract dengan mock, security | Tidak | Semua perubahan relevan |
| T2 | Engine unit/CPU-only fixture, local file streams, SQLite migration | Tidak | Sebelum provider live |
| T3 | Modal non-GPU account/deployment smoke | **Ya, dengan izin** | Sebelum GPU test |
| T4 | Modal real GPU Native & Universal, artifact | **Ya, dengan izin** | Sebelum klaim runnable |
| T5 | Modal parallel/async/recovery/cold-start/end-to-end | **Ya, dengan izin** | Sebelum rilis publik fitur itu |
| T6 | RunPod/Vast/Lambda/SSH contract mocks | Tidak | Saat adapter dibuat |
| T7 | Live provider selain Modal | **DILARANG dalam scope saat ini** | Baru setelah keputusan baru |

Tidak ada `cargo test` yang tanpa sengaja menyalakan GPU. Tandai live test `#[ignore]`, feature flag terpisah (`live-modal`), dan gunakan perintah eksplisit dengan langkah persetujuan tersendiri. Jangan memalsukan statistik latency dari mock.

## 1. Entry criteria sebelum implementasi/pengujian

- Source spec dan test plan sinkron; dua file berada di root repository pada saat user mengizinkan upload.
- Memiliki `AGENTS.md` yang menyatakan: no cloud/GitHub changes by default; source and test traceability.
- Secret scanning, `.gitignore` untuk provider/HF tokens, lockfile deps dan versi toolchain.
- Kontrak dan fixtures tidak berisi API token asli, data pengguna sungguhan, gambar pribadi, atau signed URL.
- Modal account/profile/workspace yang digunakan untuk live test adalah **akun pemilik yang memberi izin**, bukan maintainer shared account.
- Test harness local MCP dapat diluncurkan secara independen dari satu brand agent.

## 2. Test IDs — model approval, harness, account ownership (P0)

| ID | Skenario | Bukti lulus |
|---|---|---|
| H01 | `tools/list` pada MCP stdio | JSON-RPC valid; stdout hanya protokol |
| H02 | Parameter MCP salah/unknown | Typed error; tidak panic, tanpa GPU |
| H03 | Agent meminta generate tanpa model approval | `MODEL_APPROVAL_REQUIRED`, no submit |
| H04 | Agent menyisipkan `approved=true` dalam tool request | Ditolak, grant tidak diciptakan |
| H05 | Agent mencetak teks “user approved” | Tidak dipercaya, masih `PENDING` |
| H06 | Pengguna menolak model | Status `DENIED`, tidak dapat generate |
| H07 | User approve SHA A, HF `main` berubah ke SHA B | SHA B ditolak, SHA A tetap terikat grant |
| H08 | Agent/Router mencoba mengganti model A menjadi B | Ditolak tanpa human approval B |
| H09 | Model approved untuk task A dipakai task B | Ditolak jika task B tidak approved |
| H10 | Grant expired/revoked | Tidak ada submit baru; inflight semantics jelas |
| H11 | Agent mencoba approve dari stdin MCP | Tidak boleh ada path privileged approval |
| H12 | Terminal headless/no TTY | Tidak auto approve; kode penolakan jelas |
| H13 | Provider profile A diganti diam-diam ke B | Tidak ada operasi pada B; fail closed |
| H14 | Model approval dipakai untuk create GPU resource | Ditolak tanpa approval resource terpisah |
| H15 | Profile lain menggunakan token salah | Error autentikasi, tidak fallback ke profile lain |
| H16 | Agent cloud-only mencoba config executable lokal tidak tersedia | Diagnosis environment, bukan klaim berhasil |
| H17 | `harness configure` file existing | Backup, preserve options lain, atomic write, rollback |
| H18 | Harness client versi berbeda | Matrix tested/untested/unsupported sesuai bukti |

## 3. Provider Adapter contract tests — offline, wajib setiap adapter

| ID | Contract | Expected |
|---|---|---|
| P01 | `capabilities()` multi mode | Tri-state; unknown tidak dianggap supported |
| P02 | `plan()` read only | Zero create/deploy/submit side effect |
| P03 | `provision()` tanpa resource grant | Ditolak |
| P04 | `submit()` tanpa exact approval/grant | Ditolak |
| P05 | Salah provider/account/workspace/environment | Fail closed |
| P06 | Endpoint URL/host dari model card | Tidak dipakai sebagai compute endpoint |
| P07 | Network timeout sebelum submit | Retry jika aman |
| P08 | Timeout setelah provider menerima submit | Reconcile, tidak duplicate job |
| P09 | Provider tidak support cancellation | `unsupported`/`uncertain`, tidak fake cancelled |
| P10 | Provider API return 429/503/5xx | Backoff + jitter, deadline bounded |
| P11 | `cleanup --dry-run` | No destructive calls, resource scope akurat |
| P12 | Cleanup resource milik aplikasi lain | Ditolak |
| P13 | Feature pod `scale_to_zero` di provider tanpa dukungan | `manual_stop|terminate_required|unknown`, bukan automatic |
| P14 | Provider adapter not implemented | `PROVIDER_NOT_IMPLEMENTED` bukan success |
| P15 | Wrong account credentials not leaked | Redacted log/errors |
| P16 | Provider slot GPU tidak tersedia | Typed error, tidak berubah provider diam-diam |
| P17 | Provider API schema berubah | Fail closed, visible compatibility error |
| P18 | Repo user A dan user B | Tidak shared storage/credentials/namespace |

Implementasi T6 memakai **mock API recorder** untuk RunPod Serverless, RunPod Pods, Vast.ai Serverless, Vast.ai Instances, Lambda Cloud, generic SSH. Test memverifikasi interface dan error; status hasil = `contract-only / not live verified`.

## 4. Model discovery/runtime routing/supply-chain tests

| ID | Skenario | Expected |
|---|---|---|
| M01 | HF model URL valid | Canonical model ID + immutable SHA |
| M02 | URL HTTP/localhost/private/link-local | SSRF blocked |
| M03 | Redirect berbahaya setelah HEAD/GET | Blocked; no credential leakage |
| M04 | URL `../` encoded/traversal/symlink | Rejected |
| M05 | Malicious model card prompt injection | Metadata only, no shell instructions executed |
| M06 | Model gated/private | Access check, user source token optional; no bypass |
| M07 | Pickle-only weights | Not auto-loaded, safe unsupported policy |
| M08 | `trust_remote_code=True` required | Denied by default |
| M09 | Model repo task label saja mengklaim image gen | Tidak diberi status `verified` |
| M10 | Runtime family cocok tetapi image digest/checkpoint mismatch | Tidak dianggap verified |
| M11 | Rust engine gagal; Universal tidak compatible | Error, tanpa fake output |
| M12 | Rust gagal; Universal compatible, model SHA sama | Fallback aman dan idempotent |
| M13 | Router mencoba provider lain otomatis | Ditolak tanpa user selection/resource consent |
| M14 | Model license restriksi/penerimaan syarat | Status restricted/approval info, jangan otomatis bypass |
| M15 | Manifest invalid/tampered | Reject dan traceable error |
| M16 | GPU VRAM kebutuhan lebih tinggi dari resource | Planner gagal sebelum model load |

## 5. Data, media, SQLite, cache, artifact tests

| ID | Skenario | Expected |
|---|---|---|
| D01 | Input media kecil valid | Type, SHA, bytes correct |
| D02 | Video besar dikirim melalui MCP JSON | Gunakan file ref/staging; tidak inline base64 besar |
| D03 | MIME palsu/codec rusak/decode bomb | Rejected safely |
| D04 | Input path traversal / symlink outside allowed scope | Rejected |
| D05 | Upload terputus di tengah | Resumable jika supported, otherwise clean retry |
| D06 | Dua worker download checkpoint sama | No corrupt final cache; SHA verified |
| D07 | Incomplete cache file dari crash | Tidak dipakai sebagai verified cached weights |
| D08 | Volume commit/reload race | No incorrect visibility claim, data integrity verified |
| D09 | Dua job menulis output bersamaan | Unique paths; no cross-job overwrite |
| D10 | Output hash berbeda atau file terpotong | `ARTIFACT_HASH_MISMATCH`, no COMPLETED |
| D11 | Expired remote artifact | Explicit expiry; actionable error |
| D12 | SQLite crash/restart | Recovery/migrations & WAL integrity |
| D13 | Revoke grant while running | No new submit; inflight semantics consistent |
| D14 | Model source token in logs or traces | No plaintext secret |
| D15 | Cleanup user files dan resource | Preview/confirmation; no unrelated deletion |

## 6. Job async, parallel, cold-start, cancellation

| ID | Skenario | Expected |
|---|---|---|
| J01 | Job short sync | Correct final status/artifact |
| J02 | Worker scale-to-zero lalu menerima request | Cold-start recovery, no duplicate |
| J03 | Remote accepts job then HTTP timeout | Idempotent reconcile |
| J04 | Bridge/harness mati ketika video berlangsung | Durable remote job id; restart/poll/fetch succeeds bila feature advertised |
| J05 | Cancel running async job | `requested/confirmed/uncertain` accurately |
| J06 | Cancel complete job | Stable terminal result, no accidental delete |
| J07 | 10 jobs beda model/runtimes | Concurrent where safe, each correct model SHA |
| J08 | 10 jobs sama model | Per-worker VRAM-aware batching/scale, no OOM storm |
| J09 | Model requires single request per worker | Admission enforces 1 per worker, replicas may scale |
| J10 | GPU unavailable / quota exhausted | Typed error, no alternate provider silently |
| J11 | Long model initialization | Startup configured, no premature success/failure |
| J12 | Remote result expires | Explicit retention warning, local index remains honest |
| J13 | Instance manual stop/terminate lifecycle mocked | No auto zero claim; resource status report |
| J14 | Inference exception mid-stream | Clean failed job/artifact cleanup, no secret trace |
| J15 | Crash after completion before local status write | Reconciliation recovers terminal status |

## 7. Modal-only live test procedure (T3–T5; terpisah dari CI default)

### Prasyarat eksplisit

1. User telah menyetujui profile/workspace/environment Modal yang dipakai, image digest, model URL+revision, jenis operasi resource dan workload GPU; izin model dan resource terpisah.
2. User telah mengaktifkan credential lokal Modal pada profile test yang mereka miliki. Test runner tidak menerima token lewat commit, PR, issue atau chat.
3. Tidak ada deployment otomatis dari `git clone`, build, install, `cargo test`, atau GitHub Actions standar.
4. Run live test harus berupa command khusus misalnya `cargo test --features live-modal -- --ignored --nocapture` **setelah** prompt interaktif user dan capability checks. Jangan menggunakan flag ini di CI normal.
5. Semua hasil kerja diisolasi namespace `cmb-test-<installation-id>-<run-id>`; hapus **hanya** resource test milik user setelah konfirmasi. Handle resource yang gagal dibersihkan dalam laporan.

### Urutan test terkontrol

**T3 — Modal non-GPU**

- [ ] `modal` CLI tersedia dan versi tercatat; user identity, workspace, env diverifikasi secara real.
- [ ] `plan` dapat memperlihatkan resource tanpa membuatnya.
- [ ] Private test worker autentikasi: permintaan tanpa/keliru proxy token ditolak.
- [ ] Server idle → 0 container (sesuai observabilitas provider) → request → bisa berjalan; retry 503 tidak membuat job duplikat.
- [ ] Secret redaction dan cleanup `dry-run` bekerja.

**T4 — Native GPU real inference**

- [ ] Exact approved image model/revision SHA dapat load dan menjalankan inferensi Native Engine dari worker Modal.
- [ ] Catat runtime image digest, GPU type, provider resource ID, anonymous job ID, hash artifact dan durasi per tahap.
- [ ] Hasil gambar benar-benar berbeda dari mocked fixture; tidak boleh mengaku lulus dengan response dummy.
- [ ] Artifact diambil ke local dan hash diverifikasi.

**T4 — Universal GPU real inference**

- [ ] Family runtime Python/PyTorch menjalankan satu model image sesuai SHA dan license.
- [ ] Runtime image dipin digest, output terverifikasi, tidak ada arbitrary remote code.
- [ ] Rust bridge tetap control plane; worker tidak memiliki provisioning token pengguna.

**T5 — Async + parallel + scale down**

- [ ] Jalur async teruji lewat provider-supported remote handle, bukan detached thread. Restart bridge, lanjutkan status/fetch.
- [ ] Eksekusi paralel pada beberapa job; catat per-worker concurrency, VRAM, OOM behavior dan artefak unik.
- [ ] Idle setelah selesai → 0 workers (jika Modal mode yang dipakai mendukung), cache retained bila dikonfigurasi.
- [ ] Re-run setelah cold idle untuk mengukur end-to-end.

**Test video/music:** hanya dicentang ketika adapter audio/video sungguhan telah bekerja; image success tidak membuktikan video/music compatibility. Jangan jalankan model yang membutuhkan izin HF gated tanpa user authorization.

### Contoh perintah (desain target, jangan diasumsikan sudah ada)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# HANYA setelah kode mendukung feature + persetujuan pengguna:
cargo test -p bridge-provider-modal --features live-modal -- --ignored --nocapture
```

## 8. Benchmark kecepatan (pengukuran yang wajib)

- Bandingkan *cold run* dari 0 workers dan *warm run* dengan model+runtime sama.
- Laporkan per tahap: metadata discovery, planning, provider submit, provisioning/cold start, download/cache, model load, GPU inference, encoding, output staging, download, total.
- Catat variabel: GPU type, region, container image digest, model SHA, image resolution/steps/seed, input size, cache cold/warm, concurrency, timeouts, client OS/harness version.
- Minimal laporkan `n`, median/p50, p95 jika sampel mencukupi, min/max, error rate, throughput dan job completion ratio. Jangan menyimpulkan dari satu sampel atau mencampur model/hardware berbeda.
- Tujuan: temukan bottleneck aktual. Target angka latency tidak ditetapkan tanpa baseline nyata; scale-to-zero mengandung cold-start tradeoff.

## 9. Compatibility matrix yang harus dihasilkan Codex

```text
HARNESS MATRIX
client | version | OS | stdio tool discovery | model inspect | approval request
       | generation (if tested) | status | cleanup | status(tested/unverified)

PROVIDER MATRIX
provider | mode(serverless/pod/vm/ssh) | API version | authenticated |
capabilities | provision | async | cancel | artifact | cleanup |
scale-to-zero mode | live-tested? | evidence

MODEL/RUNTIME MATRIX
model source & SHA | license | task | engine | image digest | GPU profile |
parameters | output checksum evidence | measured latency | status
```

Pada fase saat ini, hanya `Modal` boleh `live-tested`. RunPod, Vast.ai, Lambda, SSH = `interface-only/contract-tested`, tidak boleh ditampilkan sebagai integrasi berjalan.

## 10. Security tests tambahan (tidak opsional)

- Fuzz parsers model URL, metadata, HTTP status body, JSON schema, provider capability, SQLite migrations, resource IDs, file paths dan input MIME.
- SAST/dependency advisory/license scan untuk Cargo/Python runtime/image; pin dependencies dan lockfile.
- Uji arbitrary Python `trust_remote_code`, pickle/checkpoint injection, malicious model `README` directions, unsafe shell interp, exfiltration via error logs.
- Uji local user permission limitation: agent shell dengan hak OS sama mungkin dapat memalsukan approval; dokumentasikan batas kepercayaan, jangan mengklaim perlindungan mutlak.
- Pastikan **tidak ada maintainer telemetry** / outbound ke endpoint proyek kecuali update metadata yang diaktifkan user. Hanya koneksi ke provider compute, model source, dan image registry yang diperlukan.
- Audit source archives dan release artifacts agar tidak mengandung tokens, private datasets atau media user.

## 11. Release gates: GO / NO-GO

### GO untuk development lokal

- Semua T0/T1 relevant pass, no secrets, no automatic cloud operation, provider adapter trait dan approval state machine nyata.

### GO untuk Modal live smoke

- Ada izin pengguna untuk operasi nyata; exact workspace terikat; approved model SHA, validated worker auth, cleanup plan; live test command terisolasi dan bisa dibatalkan.

### GO untuk publisitas "Modal image generation works"

- Dua engine (Native & Universal) benar-benar menghasilkan gambar dari GPU Modal, hasil hash diverifikasi, model+SHA+runtime digest+GPU ID direkam, error/fallback tests pass.

### GO untuk "video/music async works"

- Model kategori terkait benar-benar generate media, durable remote handle survive local restart, cancellation/recovery/retention behavior teruji.

### GO untuk "multi-provider works"

- BUKAN tahap sekarang. Harus menjalani credentialed, real workload dan lifecycle/cleanup tests **di setiap provider** yang diklaim runnable, bukan hanya mock trait.

### NO-GO untuk rilis publik jika

- Agent dapat approve model atau switch model/provider tanpa pengguna.
- Satu profile dapat mengirim job ke workspace lain.
- Endpoint GPU publik tanpa auth.
- Model belum kompatibel dipromosikan sebagai verified.
- Timeout menyebabkan duplicate expensive GPU jobs atau job kehilangan status tanpa warning.
- Hasil output hash salah, storage corrupt, atau media pengguna bocor.
- Konfigurasi default membuat cloud/GPU resource tanpa consent.
- Mengklaim semua providers/pods/VM scale-to-zero otomatis.

## 12. Evidence bundle yang harus diserahkan Codex

```text
evidence/
  date-and-commit.txt             # git SHA, build tool versions
  contracts-results.md             # test ID -> PASS/FAIL/NOT RUN
  security-results.md              # malicious input and secrets handling
  compatibility-matrix.md          # harness/provider/runtime states
  performance-modal.md             # real timings/conditions only
  modal-real-inference.md          # job metadata + artifact hash; no secrets
  scale-to-zero-modal.md           # provider-observed active/idle counts
  async-recovery.md                # restart/cancel evidence if claimed
  resources-cleanup.md             # all created resource IDs and final state
  known-limitations.md             # unimplemented models/providers/features
```

Artifact/media files berisi data pribadi jangan dimasukkan ke public evidence otomatis. Sanitasi URLs, bearer credentials, prompts dan model inputs dari laporan publik.

## 13. Testing work order untuk Codex

1. Baca `Cloud-Media-Bridge.md` dan dokumen ini, buat `traceability.csv` ID requirement → file kode → test ID → status/evidence.
2. Implementasikan T0–T2 dulu, menggunakan mock `ComputeProvider`, tidak menjalankan real GPU.
3. Tambahkan implementation Modal hanya setelah preflight identity/security/approval. Jangan membuat real resources tanpa instruksi pengguna yang eksplisit.
4. Jalankan T3–T5 khusus pada Modal setelah izin terpisah; jangan melakukan live integration untuk RunPod/Vast/Lambda/SSH sekarang.
5. Catat fail, reproduksi bug, perbaiki, rerun dan simpan evidence redacted. Jangan mengubah ekspektasi tes hanya agar lolos.
6. Buat report ringkas GO/NO-GO berdasarkan bukti, bukan klaim keberhasilan. Laporkan blokir nyata; provider/test yang belum teruji selalu diberi label `NOT RUN`.
