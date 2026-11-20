# Dependency maintenance

Run `pnpm check:dependencies` from the repository root. This checks unused and
undeclared packages, exercises Capacitor template extraction, and audits the full
JavaScript lockfile. A production-only audit is also available with
`pnpm audit --prod`.

For Rust, install cargo-machete 0.9.2 and cargo-audit 0.22.2, then run
`bash scripts/check-rust-dependencies.sh` from the repository root. CI downloads
their official Linux x86_64 release archives using pinned SHA-256 checksums. The
Rust check audits vulnerabilities and rejects unsound or yanked packages, while
leaving maintenance notices visible.

CI runs both checks on pull requests. Keep the lockfiles committed and use frozen
pnpm installs and locked Cargo commands. Refresh advisories when reviewing a
dependency update; a passing historical check is not a current security assessment.

## Reviewed exceptions

JavaScript auditing has no advisory exceptions. GitHub withdrew
[GHSA-gcqx-w48m-w2h4](https://github.com/advisories/GHSA-gcqx-w48m-w2h4)
on November 16, 2026; the obsolete deepmerge exception was removed after a fresh
full-lockfile audit reported zero vulnerabilities on November 20.

Reviewed on 2026-10-29:

| Finding                                                                                                                   | Dependency path and reachability                                                                                                                                                                                                                                        | Enforcement                                                                                                                                                                                                           |
| ------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html), RSA timing side channel                       | SQLx's lockfile includes its optional MySQL driver and `rsa` even though this application enables PostgreSQL only. `rsa` is absent from the enabled dependency graph, including development/build dependencies and all targets.                                         | Before ignoring this unpatched lockfile finding, the Rust check fails if `rsa` appears in any enabled graph. Remove the exception or resolve the advisory before enabling MySQL or another RSA consumer.              |
| [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html), unmaintained `paste`                          | Compile-time macros in the image library's EXR/AVIF codec dependencies. This is a maintenance notice, not a reported vulnerability.                                                                                                                                     | The notice remains visible. Keep the image formats supported by the existing upload API; review upstream replacement work during image updates.                                                                       |

Do not expand these exceptions merely to obtain a passing check. Review the actual
caller, enabled features, input source, and upstream fix first.

## Compatibility decisions

- Override vulnerable `glob` 9 through 14.0.1 to 14.0.2 or later and `minimatch`
  7 through 11.0.0 to 11.0.1 or later. These fix
  [GHSA-3g3m-2fjr-6pw4](https://github.com/advisories/GHSA-3g3m-2fjr-6pw4) and
  [GHSA-g3hv-75wg-m249](https://github.com/advisories/GHSA-g3hv-75wg-m249).
  The affected callers are Capacitor's rimraf cleanup, PWA asset discovery, and
  TypeScript ESLint configuration matching. Test real cleanup, lint, and PWA
  production builds when updating these major-version overrides.
- Keep Rust/Axum, React 18, Vite 6, and Capacitor 6. Update compatible releases and
  their lockfile dependencies. React Router 7 and Zod 4 are the deliberate major
  updates required for their available security fixes; navigation, validation,
  browser, and type checks cover their current usage.
- Use pnpm 11's bulk advisory endpoint; older pnpm clients rely on npm's retired
  legacy audit endpoint. Corepack selects the exact version for development,
  CI, Vercel, and Docker. Overrides and patches live in `pnpm-workspace.yaml`.
  Dependency lifecycle scripts are blocked unless explicitly allowed there;
  esbuild needs its native executable setup. Keep pnpm's release-age protection
  when refreshing the lockfile; the four Capacitor 6.2.3 security releases have
  explicit version exceptions. Strict mode prevents automatic new exceptions.
- Remove unused `aws-config`, `aws-credential-types`, `base64`, and `mockall`.
  `tower` is only a test dependency. Restrict Tokio, Axum, tower-http, and SQLx to
  the features used by the application and tests. Keep all existing image formats.
- The S3 client needs SigV4 and its modern HTTPS connector. Disable unused SigV4a
  and the legacy TLS connector. JWTs still use HS256, backed by the already-used
  AWS-LC implementation instead of enabling unused Rust RSA/ECDSA implementations.
  An independently signed token fixture checks compatibility with existing keys.
- Pin `ort` to `=2.0.0-rc.11`. A caret requirement on a prerelease can select a
  newer release candidate with a different native API. Keep ONNX Runtime 1.23.2
  compatible and run `cargo run --locked --example onnx_smoke`. CI and the API
  image use the checksum-verified native installer and explicit dynamic loading.
  The Containers workflow validates both final images, including real inference.
  The installer extracts Microsoft's official NuGet CPU package because the
  GitHub release archive returns 404. Its Linux x64 library has the same SHA-256
  as the previously verified 1.23.2 binary; Linux arm64 comes from the same pinned
  package. The full package checksum is verified before extraction. Linux builds
  need `unzip`; the API builder image installs it.
- Capacitor's CLI 6.2.3 still declares vulnerable `tar` 6. Override only that
  dependency to patched `tar` 7 and apply the small CommonJS import adaptation in
  `patches/@capacitor__cli@6.2.3.patch`. Its real template extractor is exercised
  by `apps/mobile/scripts/dependency-smoke.cjs`. Reassess and remove the patch when
  upgrading the CLI. This preserves existing native platform requirements;
  Android/iOS compilation still requires their respective SDKs.
- Knip scans browser smoke scripts as entrypoints, so Playwright and the root Vite
  fixture dependency are visible. Capacitor platform/plugin packages are retained
  because the native CLI discovers them from installed packages; Java/iOS loading
  is outside Knip's TypeScript import graph. Unused mobile React/plugin-react and
  UI icon declarations are removed. ESLint's directly imported `@eslint/js` is
  declared explicitly.
