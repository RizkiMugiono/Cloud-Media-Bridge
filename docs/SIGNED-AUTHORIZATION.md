# Independent signed approval

The user signs an exact plan and operation using a private OpenSSH Ed25519 key kept outside the agent and bridge. The executor must control its own OS identity, credentials, trusted verifier, enrolled public key, and persistent replay directory. Same-user environment variables are not an independent trust boundary.

Configure absolute paths in the trusted executor environment: CMB_AUTH_SSH_KEYGEN, CMB_AUTH_ALLOWED_SIGNERS (allowed-signers file), CMB_AUTH_LEDGER (existing private directory), and CMB_AUTH_PRINCIPAL. Do not accept trust-root changes from agents.

1. Create operation.json containing operation, resource_id (null for deploy), and input_sha256 (required for generate, null otherwise).
2. Run authorization challenge SESSION operation.json. Save sign_exact_utf8 from the response as grant.json without BOM or newline. Review the full plan, digest, exact operation, cost, identity and expiration before signing.
3. On the independent user device run ssh-keygen -Y sign -f PRIVATE_KEY -n cloud-media-bridge-v1 grant.json. Transfer only grant.json and grant.json.sig.
4. authorization verify SESSION grant.json grant.json.sig operation.json checks a grant without consuming it. authorization consume records a one-shot marker for an independently controlled external executor, immediately before dispatch; it does not call a provider.
5. The built-in deploy/generate/stop commands use CMB_AUTH_GRANT_FILE and CMB_AUTH_SIGNATURE_FILE and consume before remote dispatch. Do not pre-consume for built-in commands. Deployment markers are unique per plan even if the same plan is signed again. Other operations are unique per grant. Never delete replay markers.

The release gate remains closed. A signature does not prove account/offer/runtime correctness or independently establish the trust topology. Failed dispatch is uncertain; never blindly retry. Terminate, suspend and scale-to-zero signature scopes exist but their native dispatchers are not implemented.
