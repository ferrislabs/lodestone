import { PlusIcon, Trash2Icon } from 'lucide-react'
import {
	type FormContext,
	removeAt,
	updateAt,
} from '#/components/schema-form/context'
import { FieldErrors, FieldFrame } from '#/components/schema-form/field-frame'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Switch } from '#/components/ui/switch'
import { Textarea } from '#/components/ui/textarea'
import {
	appendIn,
	classify,
	defaultValue,
	enabledValue,
	fieldId,
	formatPath,
	genericPath,
	isOptionalProperty,
	type JsonValue,
	labelFor,
	type Node,
	type Property,
	propertiesOf,
	resolve,
	type Schema,
	type Segment,
	unwrapNullable,
	variantIndex,
} from '#/lib/schema-form'

const SELECT_CLASS =
	'h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-invalid:border-destructive dark:bg-input/30'

interface ValueProps {
	context: FormContext
	schema: Schema
	path: readonly Segment[]
	value: JsonValue | undefined
	label: string
	required: boolean
}

function problemsAt(context: FormContext, path: readonly Segment[]) {
	const target = formatPath(path)
	return context.problems.filter((problem) => problem.path === target)
}

function isScalar(node: Node): boolean {
	return (
		node.kind === 'string' ||
		node.kind === 'number' ||
		node.kind === 'boolean' ||
		node.kind === 'enum'
	)
}

function describedBy(id: string, invalid: boolean): string | undefined {
	return invalid ? `${id}-error` : undefined
}

function ScalarInput({
	context,
	node,
	path,
	value,
	label,
	rowLabel,
}: ValueProps & { node: Node; rowLabel?: string }) {
	const id = fieldId(path)
	const invalid = problemsAt(context, path).length > 0
	if (node.kind === 'boolean') {
		return (
			<Switch
				id={id}
				aria-label={label}
				checked={value === true}
				onCheckedChange={(checked) => updateAt(context, path, checked)}
			/>
		)
	}
	if (node.kind === 'enum') {
		return (
			<select
				id={id}
				aria-label={rowLabel}
				className={SELECT_CLASS}
				aria-invalid={invalid}
				aria-describedby={describedBy(id, invalid)}
				value={String(value ?? '')}
				onChange={(event) => {
					const option = node.options.find(
						(candidate) => String(candidate) === event.target.value,
					)
					updateAt(context, path, option ?? event.target.value)
				}}
			>
				{node.options.map((option) => (
					<option key={String(option)} value={String(option)}>
						{String(option)}
					</option>
				))}
			</select>
		)
	}
	if (node.kind === 'number') {
		return (
			<Input
				id={id}
				type="number"
				aria-label={rowLabel}
				inputMode={node.integer ? 'numeric' : 'decimal'}
				step={node.integer ? 1 : 'any'}
				aria-invalid={invalid}
				aria-describedby={describedBy(id, invalid)}
				value={typeof value === 'number' ? value : ''}
				onChange={(event) =>
					updateAt(
						context,
						path,
						event.target.value === '' ? null : Number(event.target.value),
					)
				}
			/>
		)
	}
	const text = typeof value === 'string' ? value : ''
	if (context.hints.textarea.includes(genericPath(path))) {
		return (
			<Textarea
				id={id}
				rows={4}
				aria-label={rowLabel}
				aria-invalid={invalid}
				aria-describedby={describedBy(id, invalid)}
				value={text}
				onChange={(event) => updateAt(context, path, event.target.value)}
			/>
		)
	}
	return (
		<Input
			id={id}
			aria-label={rowLabel}
			aria-invalid={invalid}
			aria-describedby={describedBy(id, invalid)}
			value={text}
			onChange={(event) => updateAt(context, path, event.target.value)}
		/>
	)
}

function ScalarRows({
	context,
	items,
	path,
	value,
	label,
}: {
	context: FormContext
	items: Schema
	path: readonly Segment[]
	value: readonly JsonValue[]
	label: string
}) {
	const node = classify(context.root, items)
	return (
		<ul className="flex flex-col gap-2">
			{value.map((entry, index) => {
				const itemPath = [...path, index]
				const key = formatPath(itemPath)
				return (
					<li key={key} data-row={key} className="flex flex-col gap-1">
						<div className="flex items-center gap-2">
							<ScalarInput
								context={context}
								node={node}
								schema={items}
								path={itemPath}
								value={entry}
								label={label}
								rowLabel={`${label} ${index + 1}`}
								required
							/>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								aria-label={`Remove ${label} ${index + 1}`}
								onClick={() => removeAt(context, itemPath)}
							>
								<Trash2Icon />
							</Button>
						</div>
						<FieldErrors
							id={fieldId(itemPath)}
							problems={problemsAt(context, itemPath)}
						/>
					</li>
				)
			})}
		</ul>
	)
}

function CardRows({
	context,
	items,
	path,
	value,
	label,
}: {
	context: FormContext
	items: Schema
	path: readonly Segment[]
	value: readonly JsonValue[]
	label: string
}) {
	const title = resolve(context.root, items).title ?? label
	return (
		<ul className="flex flex-col gap-3">
			{value.map((entry, index) => {
				const itemPath = [...path, index]
				return (
					<li
						key={formatPath(itemPath)}
						data-row={formatPath(itemPath)}
						className="flex flex-col gap-3 rounded-md border bg-muted/20 p-3"
					>
						<div className="flex items-center justify-between">
							<p className="text-sm font-medium">
								{title} {index + 1}
							</p>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								aria-label={`Remove ${title} ${index + 1}`}
								onClick={() => removeAt(context, itemPath)}
							>
								<Trash2Icon />
								Remove
							</Button>
						</div>
						<FieldErrors
							id={fieldId(itemPath)}
							problems={problemsAt(context, itemPath)}
						/>
						<ValueField
							context={context}
							schema={items}
							path={itemPath}
							value={entry}
							label={title}
							required
						/>
					</li>
				)
			})}
		</ul>
	)
}

function focusRow(row: string) {
	requestAnimationFrame(() => {
		document
			.querySelector<HTMLElement>(
				`[data-row="${row}"] :is(input, textarea, select)`,
			)
			?.focus()
	})
}

function ArrayField({
	context,
	items,
	path,
	value,
	label,
}: {
	context: FormContext
	items: Schema
	path: readonly Segment[]
	value: JsonValue | undefined
	label: string
}) {
	const list = Array.isArray(value) ? value : []
	const scalar = isScalar(classify(context.root, items))
	return (
		<div className="flex flex-col gap-2">
			{scalar ? (
				<ScalarRows
					context={context}
					items={items}
					path={path}
					value={list}
					label={label}
				/>
			) : (
				<CardRows
					context={context}
					items={items}
					path={path}
					value={list}
					label={label}
				/>
			)}
			<div>
				<Button
					type="button"
					variant="outline"
					size="sm"
					aria-label={`Add ${label}`}
					onClick={() => {
						context.onChange(
							appendIn(
								context.value,
								path,
								defaultValue(context.root, items, context.hints, [...path, 0]),
							),
						)
						focusRow(formatPath([...path, list.length]))
					}}
				>
					<PlusIcon />
					Add
				</Button>
			</div>
		</div>
	)
}

function UnionField({
	context,
	node,
	path,
	value,
	label,
}: ValueProps & { node: Extract<Node, { kind: 'union' }> }) {
	const index = variantIndex(node.variants, value)
	const variant = node.variants[index]
	const id = fieldId(path)
	return (
		<div className="flex flex-col gap-3">
			<select
				id={id}
				aria-label={`${label} type`}
				className={SELECT_CLASS}
				value={index}
				onChange={(event) => {
					const next = node.variants[Number(event.target.value)]
					if (next) {
						updateAt(
							context,
							path,
							defaultValue(context.root, next.schema, context.hints, path),
						)
					}
				}}
			>
				{node.variants.map((option, position) => (
					<option key={String(option.value)} value={position}>
						{option.title}
					</option>
				))}
			</select>
			{variant ? (
				<ObjectFields
					context={context}
					schema={variant.schema}
					path={path}
					value={value}
				/>
			) : null}
		</div>
	)
}

export function ObjectFields({
	context,
	schema,
	path,
	value,
}: {
	context: FormContext
	schema: Schema
	path: readonly Segment[]
	value: JsonValue | undefined
}) {
	const record =
		typeof value === 'object' && value !== null && !Array.isArray(value)
			? value
			: {}
	return (
		<div className="flex flex-col gap-4">
			{propertiesOf(resolve(context.root, schema)).map((property) => (
				<PropertyField
					key={property.key}
					context={context}
					property={property}
					path={[...path, property.key]}
					value={record[property.key]}
				/>
			))}
		</div>
	)
}

function ValueField(props: ValueProps) {
	const { context, schema, path, value, label } = props
	const node = classify(context.root, schema)
	switch (node.kind) {
		case 'object':
			return (
				<ObjectFields
					context={context}
					schema={node.schema}
					path={path}
					value={value}
				/>
			)
		case 'array':
			return (
				<ArrayField
					context={context}
					items={node.items}
					path={path}
					value={value}
					label={label}
				/>
			)
		case 'union':
			return <UnionField {...props} node={node} />
		case 'nullable':
			return <ValueField {...props} schema={node.inner} />
		case 'const':
		case 'unknown':
			return null
		default:
			return <ScalarInput {...props} node={node} />
	}
}

function PropertyField({
	context,
	property,
	path,
	value,
}: {
	context: FormContext
	property: Property
	path: readonly Segment[]
	value: JsonValue | undefined
}) {
	const node = classify(context.root, property.schema)
	if (node.kind === 'const') return null
	if (context.hints.hidden.includes(genericPath(path))) return null
	const label = labelFor(property.key, context.hints)
	const id = fieldId(path)
	const description = node.schema.description
	const problems = problemsAt(context, path)
	const container = node.kind === 'object' || node.kind === 'array'
	const optional = isOptionalProperty(context.root, property)

	if (optional) {
		const enabled = value !== undefined && value !== null
		const inner = unwrapNullable(context.root, property.schema)
		return (
			<div className="flex flex-col gap-2 rounded-md border p-3">
				<div className="flex items-start justify-between gap-3">
					<div className="flex flex-col gap-0.5">
						<label htmlFor={id} className="text-sm font-semibold">
							{label}
						</label>
						{description ? (
							<p className="text-xs text-muted-foreground">{description}</p>
						) : null}
					</div>
					<Switch
						id={id}
						aria-label={`Enable ${label}`}
						checked={enabled}
						onCheckedChange={(checked) => {
							if (checked) {
								updateAt(
									context,
									path,
									enabledValue(
										context.root,
										property.schema,
										context.hints,
										path,
									),
								)
							} else {
								removeAt(context, path)
							}
						}}
					/>
				</div>
				<FieldErrors id={id} problems={problems} />
				{enabled ? (
					<ValueField
						context={context}
						schema={inner}
						path={path}
						value={value}
						label={label}
						required={false}
					/>
				) : null}
			</div>
		)
	}

	const current =
		value ?? defaultValue(context.root, property.schema, context.hints, path)
	return (
		<FieldFrame
			id={id}
			label={label}
			description={description}
			required={property.required}
			problems={problems}
			container={container || node.kind === 'union'}
		>
			{container ? (
				<div className="rounded-md border p-3">
					<ValueField
						context={context}
						schema={property.schema}
						path={path}
						value={current}
						label={label}
						required={property.required}
					/>
				</div>
			) : (
				<ValueField
					context={context}
					schema={property.schema}
					path={path}
					value={current}
					label={label}
					required={property.required}
				/>
			)}
		</FieldFrame>
	)
}
