{{/* Chart name. */}}
{{- define "studio.name" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/* Release-qualified name, the usual fullname convention. A release named
`studio` (starcommand's Application) gives `studio`: its Service is
`studio.studio.svc`, where the cluster's Caddy routes. */}}
{{- define "studio.fullname" -}}
{{- if contains .Chart.Name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{/* Common labels. */}}
{{- define "studio.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
app.kubernetes.io/name: {{ include "studio.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{/* Selector labels. */}}
{{- define "studio.selectorLabels" -}}
app.kubernetes.io/name: {{ include "studio.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: web
{{- end -}}

{{/* The image: <registry>/<repository>:<tag, or the chart's appVersion>.
A repository containing "/" is a full image name and is used as it is. */}}
{{- define "studio.image" -}}
{{- $tag := .Values.image.tag | default .Chart.AppVersion -}}
{{- if contains "/" .Values.image.repository -}}
{{- printf "%s:%s" .Values.image.repository $tag -}}
{{- else -}}
{{- printf "%s/%s:%s" .Values.image.registry .Values.image.repository $tag -}}
{{- end -}}
{{- end -}}
