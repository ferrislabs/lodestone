# Deploying Lodestone with Helm

Chart: `charts/lodestone`. It deploys the API, the webapp, a migration Job run as a Helm hook, and optional routing, scraping and network policy. It never holds a secret value: secrets are referenced by name.

## Prerequisites

- Kubernetes 1.27 or later, Helm 3.
- A PostgreSQL the cluster can reach. Lodestone needs one database and nothing else (no Redis, no broker).
- An OIDC provider (FerryKey) with a client for Lodestone.
- Images published by `.github/workflows/docker.yaml`: `ghcr.io/ferrislabs/lodestone-{api,webapp}`, tagged `sha-<short-commit>`, release versions and `latest`. The workflow builds and merges manifests for `api` and `webapp` only. The chart also needs `ghcr.io/ferrislabs/lodestone-migrator` (Dockerfile target `migrator`); until the workflow publishes it, build and push it yourself.
- Optional: a Gateway API implementation (HTTPRoute), the Prometheus Operator CRDs (ServiceMonitor), a CNI enforcing NetworkPolicy.

## Secrets

Create them before installing; the migration hook runs before anything else in the release and needs the database one.

```bash
kubectl create namespace lodestone
kubectl -n lodestone create secret generic lodestone-db --from-literal=password='<database password>'
kubectl -n lodestone create secret generic lodestone-auth --from-literal=client-secret='<oidc client secret>'
kubectl -n lodestone create secret generic lodestone-llm --from-literal=encryption-key="$(openssl rand -base64 32)"
kubectl -n lodestone create secret generic lodestone-webhook \
  --from-literal=url='<https://receiver.example.com/hook>' \
  --from-literal=secret="$(openssl rand -hex 32)"
kubectl -n lodestone create secret generic lodestone-france-travail \
  --from-literal=client-id='<client id>' --from-literal=client-secret='<client secret>'
```

The database password is placed in a `postgres://` URL for the migrator: use a URL-safe password.

Reference them in values as `{name, key}`:

```yaml
secrets:
  DATABASE_PASSWORD: {name: lodestone-db, key: password}
```

A `value` key under any `secrets` entry fails the render. `DATABASE_PASSWORD`, `AUTH_CLIENT_SECRET` and `LLM_SECRETS_ENCRYPTION_KEY` are required. `WEBHOOK_SECRET` is required when `WEBHOOK_URL` is set.

## Local install on k3d

Not run by the chart or by CI; commands for a developer machine.

```bash
k3d cluster create lodestone --registry-create lodestone-registry:0.0.0.0:5001
docker build --target api      -t localhost:5001/lodestone-api:dev .
docker build --target migrator -t localhost:5001/lodestone-migrator:dev .
docker build --target webapp   -t localhost:5001/lodestone-webapp:dev .
docker push localhost:5001/lodestone-api:dev
docker push localhost:5001/lodestone-migrator:dev
docker push localhost:5001/lodestone-webapp:dev
```

Without a registry, `k3d image import <image> -c lodestone` works instead. Inside the cluster the registry is `k3d-lodestone-registry:5000`.

Run a throwaway Postgres in the cluster (or point `config.DATABASE_HOST` at the compose one through `host.k3d.internal`), create the secrets above, then:

```bash
helm install lodestone charts/lodestone -n lodestone \
  -f charts/lodestone/ci/default-values.yaml \
  --set imageRegistry=k3d-lodestone-registry:5000 \
  --set api.image.repository=lodestone-api --set api.image.tag=dev \
  --set migrator.image.repository=lodestone-migrator --set migrator.image.tag=dev \
  --set webapp.image.repository=lodestone-webapp --set webapp.image.tag=dev \
  --set config.DATABASE_HOST=<postgres host> \
  --set config.AUTH_ISSUER=<issuer url> --set webapp.config.issuerUrl=<browser reachable issuer url> \
  --set webapp.config.apiUrl=http://localhost:3456
kubectl -n lodestone port-forward svc/lodestone-webapp 8080:80 &
kubectl -n lodestone port-forward svc/lodestone-api 3456:80 &
```

The webapp is at http://localhost:8080. The release name `lodestone` contains the chart name, so resources are named `lodestone-api` and `lodestone-webapp`; with another release name they are `<release>-lodestone-*`.

## Design notes

- Probes: startup, readiness and liveness hit `/health` on the internal port 3457. The API binds the internal router to loopback by default; the chart sets `SERVER_INTERNAL_HOST=0.0.0.0` so kubelet and Prometheus can reach it. The router has no authentication: it is never routed, and `networkPolicy.enabled` restricts port 3457 to `networkPolicy.metricsFrom`.
- Shutdown: `terminationGracePeriodSeconds = JOBS_SHUTDOWN_GRACE_SECONDS + api.preStopSleepSeconds + terminationMarginSeconds` (default 20 + 5 + 10 = 35). The render fails if it reaches `JOBS_LEASE_SECONDS` (default 300): a job still running when the pod is killed would otherwise outlive its lease.
- Replicas: every replica runs the job workers. `PgJobQueue` claims with `FOR UPDATE SKIP LOCKED` and a lease, so replicas do not run the same job twice. Handlers must still be idempotent (a crash replays the job). A PodDisruptionBudget is created when `replicas > 1`.
- Webapp: the image entrypoint substitutes `API_URL`, `ISSUER_URL`, `OIDC_CLIENT_ID`, `OIDC_SCOPE` into `/usr/share/nginx/html/config.json` at start, so the chart passes environment variables rather than mounting a ConfigMap. That write is in place, so the webapp container keeps a writable root filesystem (it still runs as uid 101, non-root, no capabilities).
- Config rollout: the API pod template carries a checksum of the ConfigMap, so a config change restarts it.

## Values reference

| Key | Default | Meaning |
|---|---|---|
| `imageRegistry` | `ghcr.io` | Registry of the three images |
| `<component>.image.repository` | `ferrislabs/lodestone-{api,migrator,webapp}` | Image repository |
| `<component>.image.tag` | `""` (chart appVersion) | Pin to `sha-<short-commit>` or a release |
| `imagePullPolicy`, `imagePullSecrets` | `IfNotPresent`, `[]` | Pull settings |
| `config.*` | see values.yaml | ConfigMap entries, keys are the API env names |
| `config.DATABASE_HOST`, `AUTH_ISSUER`, `OUTBOUND_CONTACT` | `""` | Required |
| `config.JOBS_LEASE_SECONDS` / `JOBS_SHUTDOWN_GRACE_SECONDS` | `300` / `20` | Drive the termination grace |
| `secrets.<ENV_NAME>` | `{}` | `{name, key}` reference to an existing Secret |
| `terminationMarginSeconds` | `10` | Added to grace and preStop |
| `api.replicas`, `webapp.replicas` | `1` | Replica counts |
| `api.preStopSleepSeconds` | `5` | Drain delay before SIGTERM |
| `api.resources`, `migrator.resources`, `webapp.resources` | see values.yaml | Requests and limits |
| `migrator.backoffLimit` | `2` | Job retries |
| `webapp.config.{apiUrl,issuerUrl,oidcClientId,oidcScope}` | see values.yaml | Runtime config of the SPA; `issuerUrl` required |
| `podDisruptionBudget.enabled` / `maxUnavailable` | `true` / `1` | Created when replicas > 1 |
| `httpRoute.{enabled,parentRefs,hostnames,annotations}` | off | Gateway API: `/` to webapp, `/api` to API |
| `ingress.*` | off | Alternative to HTTPRoute |
| `networkPolicy.{enabled,metricsFrom}` | off | Port 3456 open, 3457 only from `metricsFrom` |
| `serviceMonitor.{enabled,namespace,interval,scrapeTimeout,labels}` | off | Scrapes `/metrics` on the internal service |
| `serviceAccount.*` | create, no token mount | ServiceAccount |

## Upgrades and migrations

`helm install` and `helm upgrade` first run the Job `<release>-migrate` (`sqlx migrate run`, hook `pre-install,pre-upgrade`, delete policy `before-hook-creation,hook-succeeded`). The Deployments roll only if it succeeds; a failed Job leaves its pod for inspection and fails the release. The Job uses no ServiceAccount and no ConfigMap, since neither exists yet on a first install: it reads the database settings from values and the password from the Secret.

Migrations are applied before the new API starts, so each migration must be compatible with the previous API version still serving during the rollout.

## Rollback

`helm rollback` restores the previous manifests and images. It does not revert migrations: the schema stays at the newer version. If the older API cannot run on it, apply the down migration by hand against the database (`sqlx migrate revert` with `DATABASE_URL`), then roll back.

## Proposed CI job (needs human review)

Not applied: `.github/workflows/**` is human-only. Add to `.github/workflows/` as `helm.yaml`. Pin actions and downloads to SHAs and checksums before merging; the values marked `<...>` are not filled in.

```yaml
name: helm

on:
  push:
    branches:
      - main
  pull_request:
    paths:
      - charts/**
      - docs/deploy/**
      - .github/workflows/helm.yaml

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  chart:
    name: helm (lint, template, kubeconform)
    permissions:
      contents: read
    runs-on: ubuntu-24.04
    steps:
      - name: checkout
        uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5

      - name: set up helm
        uses: azure/setup-helm@<sha> # v4
        with:
          version: v3.18.4

      - name: install kubeconform
        run: |
          curl -sSfL -o kubeconform.tgz https://github.com/yannh/kubeconform/releases/download/v0.7.0/kubeconform-linux-amd64.tar.gz
          echo "<sha256>  kubeconform.tgz" | sha256sum -c -
          tar -xzf kubeconform.tgz kubeconform

      - name: helm lint
        run: |
          helm lint charts/lodestone -f charts/lodestone/ci/default-values.yaml
          helm lint charts/lodestone -f charts/lodestone/ci/full-values.yaml

      - name: helm template and kubeconform
        run: |
          for values in default full; do
            helm template ci charts/lodestone -f charts/lodestone/ci/${values}-values.yaml \
              | ./kubeconform -strict -summary -ignore-missing-schemas \
                  -schema-location default \
                  -schema-location 'https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json' \
                  -kubernetes-version 1.30.0
          done

      - name: render refuses a literal secret
        run: |
          if helm template ci charts/lodestone -f charts/lodestone/ci/default-values.yaml \
            --set secrets.DATABASE_PASSWORD.value=x >/dev/null 2>&1; then
            echo "chart accepted a literal secret value"; exit 1
          fi

      - name: render refuses a grace period beyond the lease
        run: |
          if helm template ci charts/lodestone -f charts/lodestone/ci/default-values.yaml \
            --set config.JOBS_SHUTDOWN_GRACE_SECONDS=300 >/dev/null 2>&1; then
            echo "chart accepted a grace period above the lease"; exit 1
          fi
```
