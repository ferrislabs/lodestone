## Ferris Labs Web App

### API client

The typed client in `src/api/api.client.ts` and `src/api/api.tanstack.ts` is generated from the OpenAPI spec of the Rust API. No running server and no database are needed:

```bash
pnpm generate:api
```

This runs `cargo run -p api --bin openapi` (prints the spec from `ApiDoc`), writes it to `.generated/openapi.json` (git-ignored), then runs `typed-openapi`. Re-run it after any change to an API route or DTO.

Against a running server, the equivalent is:

```bash
pnpm typed-openapi http://localhost:3456/api-docs/openapi.json -o src/api/api.client.ts --tanstack=api.tanstack.ts
```

### Commands

```bash
pnpm dev      # vite dev on port 3000, API expected on VITE_API_URL (default http://localhost:3456)
pnpm check    # biome lint + format + organize imports
pnpm test     # vitest
pnpm build
```
