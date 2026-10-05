import {
	type JsonValue,
	type Problem,
	removeIn,
	type Schema,
	type Segment,
	setIn,
	type UiHints,
} from '#/lib/schema-form'

export interface FormContext {
	root: Schema
	hints: UiHints
	value: JsonValue
	problems: readonly Problem[]
	onChange: (next: JsonValue) => void
}

export function updateAt(
	context: FormContext,
	path: readonly Segment[],
	next: JsonValue,
) {
	context.onChange(setIn(context.value, path, next))
}

export function removeAt(context: FormContext, path: readonly Segment[]) {
	context.onChange(removeIn(context.value, path))
}
