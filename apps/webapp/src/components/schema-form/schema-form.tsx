import type { FormContext } from '#/components/schema-form/context'
import { ObjectFields } from '#/components/schema-form/fields'

export type { FormContext }

export function SchemaForm({ context }: { context: FormContext }) {
	return (
		<ObjectFields
			context={context}
			schema={context.root}
			path={[]}
			value={context.value}
		/>
	)
}
