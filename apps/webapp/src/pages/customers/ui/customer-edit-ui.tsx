import { Link } from '@tanstack/react-router'
import { ArrowLeft } from 'lucide-react'
import { FloatingActionBar } from '#/components/floating-action-bar'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from '#/components/ui/select'
import {
	CATEGORY_LABELS,
	type Customer,
	type CustomerCategory,
} from '#/pages/customers/types'

const AVATAR_COLOR: Record<CustomerCategory, string> = {
	artisan: 'bg-amber-500',
	sme: 'bg-blue-500',
	individual: 'bg-emerald-500',
}

const CATEGORIES: CustomerCategory[] = ['artisan', 'sme', 'individual']

interface CustomerEditUIProps {
	customer: Customer
	form: Customer
	isDirty: boolean
	changedKeys: (keyof Customer)[]
	isSaving: boolean
	onChange: (patch: Partial<Customer>) => void
	onAddressChange: (patch: Partial<Customer['address']>) => void
	onReset: () => void
	onSave: () => void
}

export function CustomerEditUI({
	customer,
	form,
	isDirty,
	changedKeys,
	isSaving,
	onChange,
	onAddressChange,
	onReset,
	onSave,
}: CustomerEditUIProps) {
	return (
		<div className="flex flex-col gap-6 p-4 pb-24 md:p-8 md:pb-28">
			<div>
				<Link
					to="/customers"
					className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
				>
					<ArrowLeft className="size-4" />
					Retour aux clients
				</Link>
			</div>

			<header className="flex items-center gap-4">
				<div
					className={`flex size-14 shrink-0 items-center justify-center rounded-2xl text-xl font-bold text-white ${
						AVATAR_COLOR[form.category]
					}`}
				>
					{form.name[0]?.toUpperCase()}
				</div>
				<div className="min-w-0">
					<h1 className="truncate text-2xl font-bold tracking-tight md:text-[28px]">
						{form.name || 'Nouveau client'}
					</h1>
					<p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
						id: {customer.id}
					</p>
				</div>
			</header>

			<div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
				<Section
					title="Identité"
					description="Informations principales du client"
					className="lg:col-span-2"
				>
					<Field
						label="Nom"
						name="name"
						value={form.name}
						onChange={(v) => onChange({ name: v })}
						changed={changedKeys.includes('name')}
					/>
					<Field
						label="Contact"
						name="contact_name"
						value={form.contact_name}
						onChange={(v) => onChange({ contact_name: v })}
						changed={changedKeys.includes('contact_name')}
					/>
					<div className="flex flex-col gap-2">
						<Label htmlFor="category">
							Catégorie
							{changedKeys.includes('category') ? <Dot /> : null}
						</Label>
						<Select
							value={form.category}
							onValueChange={(v) =>
								onChange({ category: v as CustomerCategory })
							}
						>
							<SelectTrigger id="category" className="rounded-xl">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{CATEGORIES.map((c) => (
									<SelectItem key={c} value={c}>
										{CATEGORY_LABELS[c]}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>
				</Section>

				<Section title="Coordonnées" description="Email et téléphone">
					<Field
						label="Email"
						name="email"
						type="email"
						value={form.email}
						onChange={(v) => onChange({ email: v })}
						changed={changedKeys.includes('email')}
					/>
					<Field
						label="Téléphone"
						name="phone"
						value={form.phone}
						onChange={(v) => onChange({ phone: v })}
						changed={changedKeys.includes('phone')}
					/>
				</Section>

				<Section
					title="Adresse"
					description="Localisation du client"
					className="lg:col-span-3"
				>
					<div className="grid grid-cols-1 gap-4 md:grid-cols-3">
						<Field
							label="Rue"
							name="street"
							value={form.address.street}
							onChange={(v) => onAddressChange({ street: v })}
							changed={changedKeys.includes('address')}
						/>
						<Field
							label="Ville"
							name="city"
							value={form.address.city}
							onChange={(v) => onAddressChange({ city: v })}
							changed={changedKeys.includes('address')}
						/>
						<Field
							label="Code postal"
							name="zip"
							value={form.address.zip}
							onChange={(v) => onAddressChange({ zip: v })}
							changed={changedKeys.includes('address')}
						/>
					</div>
				</Section>
			</div>

			<FloatingActionBar
				show={isDirty}
				message={
					changedKeys.length === 1
						? '1 modification non enregistrée'
						: `${changedKeys.length} modifications non enregistrées`
				}
				confirmLabel="Enregistrer"
				cancelLabel="Annuler"
				onCancel={onReset}
				onConfirm={onSave}
				isLoading={isSaving}
			/>
		</div>
	)
}

interface SectionProps {
	title: string
	description?: string
	className?: string
	children: React.ReactNode
}

function Section({
	title,
	description,
	className = '',
	children,
}: SectionProps) {
	return (
		<section className={`rounded-2xl border bg-card ${className}`}>
			<div className="border-b px-5 py-4">
				<h2 className="font-semibold">{title}</h2>
				{description ? (
					<p className="text-xs text-muted-foreground">{description}</p>
				) : null}
			</div>
			<div className="flex flex-col gap-4 p-5">{children}</div>
		</section>
	)
}

interface FieldProps {
	label: string
	name: string
	value: string
	onChange: (v: string) => void
	type?: string
	changed?: boolean
}

function Field({
	label,
	name,
	value,
	onChange,
	type = 'text',
	changed,
}: FieldProps) {
	return (
		<div className="flex flex-col gap-2">
			<Label htmlFor={name}>
				{label}
				{changed ? <Dot /> : null}
			</Label>
			<Input
				id={name}
				name={name}
				type={type}
				value={value}
				onChange={(e) => onChange(e.target.value)}
				className="rounded-xl"
			/>
		</div>
	)
}

function Dot() {
	return (
		<span
			role="img"
			aria-label="modifié"
			className="ml-1.5 inline-block size-1.5 rounded-full bg-orange-500 align-middle"
		/>
	)
}
