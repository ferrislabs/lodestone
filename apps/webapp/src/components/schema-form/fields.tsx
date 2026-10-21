import { PlusIcon, Trash2Icon } from 'lucide-react'
import {
	type FormContext,
	removeAt,
	updateAt,
} from '#/components/schema-form/context'
import { FieldErrors, FieldFrame } from '#/components/schema-form/field-frame'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { NumberChoice } from '#/components/ui/number-choice'
import { Switch } from '#/components/ui/switch'
import { TagInput } from '#/components/ui/tag-input'
import { Textarea } from '#/components/ui/textarea'
import {
	appendIn,
	classify,
	copyFor,
	defaultValue,
	enabledValue,
	fieldId,
	formatPath,
	genericPath,
	isOptionalProperty,
	type JsonValue,
	labelFor,
	type Node,
	type Problem,
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
		const range = context.hints.choices?.[genericPath(path)]
		if (range) {
			const current = typeof value === 'number' ? value : null
			return (
				<NumberChoice
					label={label}
					min={range.min}
					max={range.max}
					value={current}
					invalid={
						invalid ||
						(current !== null && (current < range.min || current > range.max))
					}
					describedBy={describedBy(id, invalid)}
					onChange={(next) => updateAt(context, path, next)}
				/>
			)
		}
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
								aria-label={context.hints.messages.removeAria(label, index + 1)}
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

function isFlatObject(root: Schema, schema: Schema): boolean {
	const node = classify(root, schema)
	if (node.kind !== 'object') return false
	return node.properties.every((property) => {
		const kind = classify(root, property.schema)
		return kind.kind === 'const' || (property.required && isScalar(kind))
	})
}

function isCompact(root: Schema, items: Schema): boolean {
	const node = classify(root, items)
	if (node.kind === 'union') {
		return node.variants.every((variant) => isFlatObject(root, variant.schema))
	}
	return isFlatObject(root, items)
}

function TagList({
	context,
	path,
	value,
	label,
}: {
	context: FormContext
	path: readonly Segment[]
	value: readonly JsonValue[]
	label: string
}) {
	const id = fieldId(path)
	const values = value.map((entry) => (typeof entry === 'string' ? entry : ''))
	const invalid: number[] = []
	const messages = new Map<string, Problem>()
	values.forEach((_, index) => {
		const found = problemsAt(context, [...path, index])
		if (found.length > 0) invalid.push(index)
		for (const problem of found) messages.set(problem.message, problem)
	})
	const errorId = `${id}-items`
	return (
		<div className="flex flex-col gap-1">
			<TagInput
				id={id}
				label={label}
				values={values}
				invalid={invalid}
				describedBy={invalid.length > 0 ? `${errorId}-error` : undefined}
				onChange={(next) => updateAt(context, path, next)}
			/>
			<FieldErrors id={errorId} problems={[...messages.values()]} />
		</div>
	)
}

function CompactFields({
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
		<>
			{propertiesOf(resolve(context.root, schema)).map((property) => {
				const node = classify(context.root, property.schema)
				if (node.kind === 'const') return null
				const fieldPath = [...path, property.key]
				const copy = copyFor(fieldPath, context.hints)
				const label = copy?.label ?? labelFor(property.key, context.hints)
				return (
					<div
						key={property.key}
						className="flex min-w-32 flex-1 flex-col gap-1"
					>
						<Label
							htmlFor={fieldId(fieldPath)}
							className="text-xs text-muted-foreground"
						>
							{label}
						</Label>
						<ScalarInput
							context={context}
							node={node}
							schema={property.schema}
							path={fieldPath}
							value={record[property.key]}
							label={label}
							required
						/>
						<FieldErrors
							id={fieldId(fieldPath)}
							problems={problemsAt(context, fieldPath)}
						/>
					</div>
				)
			})}
		</>
	)
}

function CompactRows({
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
	const title =
		copyFor([...path, 0], context.hints)?.label ??
		resolve(context.root, items).title ??
		label
	const node = classify(context.root, items)
	return (
		<ul className="flex flex-col gap-2">
			{value.map((entry, index) => {
				const itemPath = [...path, index]
				const key = formatPath(itemPath)
				const variantAt =
					node.kind === 'union' ? variantIndex(node.variants, entry) : 0
				const schema =
					node.kind === 'union'
						? (node.variants[variantAt]?.schema ?? items)
						: items
				return (
					<li key={key} data-row={key} className="flex flex-col gap-1">
						<div className="flex items-end gap-2">
							<div className="flex flex-1 flex-wrap items-end gap-2">
								{node.kind === 'union' ? (
									<select
										aria-label={context.hints.messages.typeAria(title)}
										className={`${SELECT_CLASS} w-36 flex-none`}
										value={variantAt}
										onChange={(event) => {
											const next = node.variants[Number(event.target.value)]
											if (next) {
												updateAt(
													context,
													itemPath,
													defaultValue(
														context.root,
														next.schema,
														context.hints,
														itemPath,
													),
												)
											}
										}}
									>
										{node.variants.map((option, position) => (
											<option key={String(option.value)} value={position}>
												{context.hints.variants[String(option.value)] ??
													option.title}
											</option>
										))}
									</select>
								) : null}
								<CompactFields
									context={context}
									schema={schema}
									path={itemPath}
									value={entry}
								/>
							</div>
							<Button
								type="button"
								variant="ghost"
								size="icon"
								aria-label={context.hints.messages.removeAria(title, index + 1)}
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
	const title =
		copyFor([...path, 0], context.hints)?.label ??
		resolve(context.root, items).title ??
		label
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
								aria-label={context.hints.messages.removeAria(title, index + 1)}
								onClick={() => removeAt(context, itemPath)}
							>
								<Trash2Icon />
								{context.hints.messages.remove}
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
	const itemNode = classify(context.root, items)
	if (itemNode.kind === 'string') {
		return <TagList context={context} path={path} value={list} label={label} />
	}
	const rows = {
		context,
		items,
		path,
		value: list,
		label,
	}
	const addText = copyFor([...path, 0], context.hints)?.add
	return (
		<div className="flex flex-col gap-2">
			{isScalar(itemNode) ? (
				<ScalarRows {...rows} />
			) : isCompact(context.root, items) ? (
				<CompactRows {...rows} />
			) : (
				<CardRows {...rows} />
			)}
			<div>
				<Button
					type="button"
					variant="outline"
					size="sm"
					aria-label={context.hints.messages.addAria(label)}
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
					{addText ?? context.hints.messages.add}
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
				aria-label={context.hints.messages.typeAria(label)}
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
						{context.hints.variants[String(option.value)] ?? option.title}
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

const ROW_COLUMNS: Readonly<Record<number, string>> = {
	2: 'md:grid-cols-2',
	3: 'md:grid-cols-3',
	4: 'md:grid-cols-4',
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
	const properties = propertiesOf(resolve(context.root, schema))
	const keys = new Set(properties.map((property) => property.key))
	const layout = context.hints.layout?.[genericPath(path)]
	const rowKeys = (layout?.row ?? []).filter((key) => keys.has(key))
	const foldedKeys = (layout?.collapsed?.keys ?? []).filter((key) =>
		keys.has(key),
	)
	const field = (property: Property, dense: boolean) => (
		<PropertyField
			key={property.key}
			context={context}
			property={property}
			path={[...path, property.key]}
			value={record[property.key]}
			dense={dense}
		/>
	)
	const byKey = (key: string) => properties.find((p) => p.key === key)
	const folded = foldedKeys.flatMap((key) => byKey(key) ?? [])
	const hasFoldedProblem = foldedKeys.some((key) => {
		const prefix = formatPath([...path, key])
		return context.problems.some(
			(problem) =>
				problem.path === prefix ||
				problem.path.startsWith(`${prefix}.`) ||
				problem.path.startsWith(`${prefix}[`),
		)
	})
	let rowEmitted = false
	return (
		<div className="flex flex-col gap-4">
			{properties.map((property) => {
				if (foldedKeys.includes(property.key)) return null
				if (rowKeys.includes(property.key)) {
					if (rowEmitted) return null
					rowEmitted = true
					return (
						<div
							key="row"
							className={`grid gap-3 ${ROW_COLUMNS[rowKeys.length] ?? ''}`}
						>
							{rowKeys
								.flatMap((key) => byKey(key) ?? [])
								.map((p) => field(p, rowKeys.length >= 3))}
						</div>
					)
				}
				return field(property, false)
			})}
			{layout?.collapsed && folded.length > 0 ? (
				<details
					open={hasFoldedProblem}
					className="flex flex-col gap-3 rounded-md border p-3"
				>
					<summary className="cursor-pointer text-sm font-medium">
						{layout.collapsed.title}
					</summary>
					<div className="mt-3 flex flex-col gap-4">
						{folded.map((property) => field(property, false))}
					</div>
				</details>
			) : null}
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
	dense = false,
}: {
	context: FormContext
	property: Property
	path: readonly Segment[]
	value: JsonValue | undefined
	dense?: boolean
}) {
	const node = classify(context.root, property.schema)
	if (node.kind === 'const') return null
	if (context.hints.hidden.includes(genericPath(path))) return null
	const copy = copyFor(path, context.hints)
	const label = copy?.label ?? labelFor(property.key, context.hints)
	const id = fieldId(path)
	const description = dense
		? undefined
		: (copy?.description ?? node.schema.description)
	const problems = problemsAt(context, path)
	const tags =
		node.kind === 'array' &&
		classify(context.root, node.items).kind === 'string'
	const container = (node.kind === 'object' || node.kind === 'array') && !tags
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
						aria-label={context.hints.messages.enableAria(label)}
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
			requiredTitle={context.hints.messages.required}
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
