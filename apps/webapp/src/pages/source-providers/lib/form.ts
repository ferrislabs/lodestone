import type {
	FieldValues,
	ProviderDetail,
	ProviderField,
	SaveBody,
	SecretInputs,
} from '#/pages/source-providers/lib/types'

export function settingFields(detail: ProviderDetail): ProviderField[] {
	return detail.fields.filter((field) => field.kind !== 'secret')
}

export interface Attestation {
	key: string
	label: string
	at: string
}

export function attestations(detail: ProviderDetail): Attestation[] {
	return settingFields(detail).flatMap((field) => {
		const at = detail.settings[`${field.key}_at`]
		if (field.kind !== 'bool' || !field.required) return []
		if (detail.settings[field.key] !== true || typeof at !== 'string') return []
		return [{ key: field.key, label: field.label, at }]
	})
}

export function secretFields(detail: ProviderDetail): ProviderField[] {
	return detail.fields.filter((field) => field.kind === 'secret')
}

function initialValue(field: ProviderField, stored: unknown): string | boolean {
	if (field.kind === 'bool') {
		if (typeof stored === 'boolean') return stored
		return field.default === 'true'
	}
	if (typeof stored === 'string') return stored
	if (typeof stored === 'number') return String(stored)
	return field.default ?? ''
}

export function initialValues(detail: ProviderDetail): FieldValues {
	const values: FieldValues = {}
	for (const field of settingFields(detail)) {
		values[field.key] = initialValue(field, detail.settings[field.key])
	}
	return values
}

export function buildBody(
	detail: ProviderDetail,
	values: FieldValues,
	secrets: SecretInputs,
): SaveBody {
	const settings: SaveBody['settings'] = {}
	for (const field of settingFields(detail)) {
		const value = values[field.key]
		if (typeof value === 'boolean') {
			settings[field.key] = value
			continue
		}
		const text = (value ?? '').trim()
		if (text === '') continue
		if (field.kind === 'number') {
			const number = Number(text.replace(',', '.'))
			if (Number.isFinite(number)) settings[field.key] = number
			continue
		}
		settings[field.key] = text
	}
	const replaced: Record<string, string> = {}
	for (const field of secretFields(detail)) {
		const value = secrets[field.key]
		if (value !== undefined && value.trim() !== '') {
			replaced[field.key] = value
		}
	}
	return { settings, secrets: replaced }
}

export function isDirty(
	detail: ProviderDetail,
	values: FieldValues,
	secrets: SecretInputs,
): boolean {
	const initial = initialValues(detail)
	const changed = Object.keys(initial).some(
		(key) => initial[key] !== values[key],
	)
	return changed || Object.values(secrets).some((value) => value.trim() !== '')
}

export function fieldErrors(
	fields: Readonly<Record<string, string>>,
): Record<string, string> {
	const errors: Record<string, string> = {}
	for (const [path, message] of Object.entries(fields)) {
		errors[path.startsWith('secrets.') ? path.slice('secrets.'.length) : path] =
			message
	}
	return errors
}

export function secretState(detail: ProviderDetail, key: string) {
	return detail.secrets.find((secret) => secret.key === key)
}
