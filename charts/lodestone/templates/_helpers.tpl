{{- define "lodestone.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "lodestone.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "lodestone.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "lodestone.labels" -}}
helm.sh/chart: {{ include "lodestone.chart" .ctx }}
app.kubernetes.io/name: {{ include "lodestone.name" .ctx }}
app.kubernetes.io/instance: {{ .ctx.Release.Name }}
app.kubernetes.io/component: {{ .component }}
app.kubernetes.io/part-of: lodestone
app.kubernetes.io/managed-by: {{ .ctx.Release.Service }}
{{- if .ctx.Chart.AppVersion }}
app.kubernetes.io/version: {{ .ctx.Chart.AppVersion | quote }}
{{- end }}
{{- end -}}

{{- define "lodestone.selectorLabels" -}}
app.kubernetes.io/name: {{ include "lodestone.name" .ctx }}
app.kubernetes.io/instance: {{ .ctx.Release.Name }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}

{{- define "lodestone.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "lodestone.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{- define "lodestone.image" -}}
{{- $tag := default .ctx.Chart.AppVersion .image.tag -}}
{{- printf "%s/%s:%s" .ctx.Values.imageRegistry .image.repository $tag -}}
{{- end -}}

{{- define "lodestone.guardSecrets" -}}
{{- range $name, $ref := .Values.secrets -}}
{{- if hasKey $ref "value" -}}
{{- fail (printf "secrets.%s must reference an existing Secret with {name, key}; a literal value is not allowed" $name) -}}
{{- end -}}
{{- end -}}
{{- range $required := list "DATABASE_PASSWORD" "AUTH_CLIENT_SECRET" "LLM_SECRETS_ENCRYPTION_KEY" -}}
{{- $ref := index $.Values.secrets $required -}}
{{- if not (and $ref $ref.name $ref.key) -}}
{{- fail (printf "secrets.%s is required: set {name, key} of an existing Secret" $required) -}}
{{- end -}}
{{- end -}}
{{- if and (hasKey .Values.secrets "WEBHOOK_URL") (not (hasKey .Values.secrets "WEBHOOK_SECRET")) -}}
{{- fail "secrets.WEBHOOK_SECRET is required when secrets.WEBHOOK_URL is set" -}}
{{- end -}}
{{- range $key := list "DATABASE_HOST" "AUTH_ISSUER" "OUTBOUND_CONTACT" -}}
{{- if not (index $.Values.config $key) -}}
{{- fail (printf "config.%s is required" $key) -}}
{{- end -}}
{{- end -}}
{{- if not .Values.webapp.config.issuerUrl -}}
{{- fail "webapp.config.issuerUrl is required" -}}
{{- end -}}
{{- end -}}

{{- define "lodestone.terminationGracePeriodSeconds" -}}
{{- $grace := int (default 20 .Values.config.JOBS_SHUTDOWN_GRACE_SECONDS) -}}
{{- $lease := int (default 300 .Values.config.JOBS_LEASE_SECONDS) -}}
{{- $total := add $grace (int .Values.api.preStopSleepSeconds) (int .Values.terminationMarginSeconds) -}}
{{- if ge $total $lease -}}
{{- fail (printf "terminationGracePeriodSeconds (%d = JOBS_SHUTDOWN_GRACE_SECONDS %d + preStopSleepSeconds %d + terminationMarginSeconds %d) must be lower than JOBS_LEASE_SECONDS (%d)" $total $grace (int .Values.api.preStopSleepSeconds) (int .Values.terminationMarginSeconds) $lease) -}}
{{- end -}}
{{- $total -}}
{{- end -}}

{{- define "lodestone.secretEnv" -}}
{{- range $name := keys .Values.secrets | sortAlpha }}
{{- $ref := index $.Values.secrets $name }}
- name: {{ $name }}
  valueFrom:
    secretKeyRef:
      name: {{ $ref.name }}
      key: {{ $ref.key }}
{{- end }}
{{- end -}}
