/**
 * An OpenSSH-encoded Ed25519 key pair produced by the Rust core.
 *
 * Field names follow the JavaScript conventions of the binding; the Rust record
 * uses `private_key_openssh`, `public_key_openssh` and `fingerprint_sha256`.
 */
export type GeneratedKey = {
  privateKeyOpenssh: string;
  publicKeyOpenssh: string;
  fingerprintSha256: string;
};
