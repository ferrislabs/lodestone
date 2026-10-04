import { Link } from '@tanstack/react-router'
import {
	ArrowRight,
	FileText,
	Package,
	Plus,
	Receipt,
	TrendingUp,
	Users,
} from 'lucide-react'
import { Button } from '#/components/ui/button'

interface HomeUIProps {
	userName?: string
	stats: {
		customers: number
		inventory: number
		invoices: number
		revenueMonth: number
	}
}

export function HomeUI({ userName, stats }: HomeUIProps) {
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
				<div>
					<h1 className="text-2xl font-bold tracking-tight md:text-[28px]">
						{userName ? `Bonjour, ${userName}` : 'Tableau de bord'}
					</h1>
					<p className="mt-1 text-sm text-muted-foreground">
						Voici un résumé de votre activité. Gérez vos clients, devis et
						factures en un clin d'œil.
					</p>
				</div>
				<Button
					asChild
					className="h-10 rounded-xl bg-orange-600 px-4 font-semibold text-white hover:bg-orange-700"
				>
					<Link to="/customers">
						<Plus />
						Nouveau client
					</Link>
				</Button>
			</header>

			<section className="flex flex-col gap-4">
				<p className="text-sm text-muted-foreground">Aperçu général</p>
				<div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
					<StatCard
						label="Clients"
						value={stats.customers.toString()}
						hint="Total enregistrés"
						icon={<Users className="size-4" />}
					/>
					<StatCard
						label="Stock"
						value={stats.inventory.toString()}
						hint="Articles en inventaire"
						icon={<Package className="size-4" />}
					/>
					<StatCard
						label="Factures"
						value={stats.invoices.toString()}
						hint="En attente de paiement"
						icon={<Receipt className="size-4" />}
					/>
					<StatCard
						label="CA du mois"
						value={`${stats.revenueMonth.toLocaleString('fr-FR')} €`}
						hint="Revenus générés ce mois"
						icon={<TrendingUp className="size-4" />}
					/>
				</div>
			</section>

			<section className="grid grid-cols-1 gap-4 lg:grid-cols-3">
				<div className="rounded-2xl border bg-card lg:col-span-2">
					<div className="flex items-center justify-between border-b px-5 py-4">
						<div>
							<h2 className="font-semibold">Activité récente</h2>
							<p className="text-xs text-muted-foreground">
								Derniers événements de votre espace
							</p>
						</div>
					</div>
					<ul className="divide-y">
						<ActivityRow
							letter="M"
							color="bg-emerald-500"
							title="Marie Leroy"
							badge="particulier"
							badgeColor="bg-emerald-50 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300"
							subtitle="Nouveau client ajouté · Bordeaux"
							meta="il y a 2 j"
						/>
						<ActivityRow
							letter="P"
							color="bg-amber-500"
							title="Plomberie Dupont"
							badge="artisan"
							badgeColor="bg-amber-50 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300"
							subtitle="Fiche client mise à jour · Lyon"
							meta="il y a 5 j"
						/>
						<ActivityRow
							letter="C"
							color="bg-blue-500"
							title="Cloud IAM"
							badge="PME"
							badgeColor="bg-blue-50 text-blue-700 dark:bg-blue-500/15 dark:text-blue-300"
							subtitle="Nouveau client ajouté · Toulouse"
							meta="il y a 1 sem"
						/>
					</ul>
				</div>

				<div className="rounded-2xl border bg-card">
					<div className="border-b px-5 py-4">
						<h2 className="font-semibold">Raccourcis</h2>
						<p className="text-xs text-muted-foreground">
							Accès rapide aux actions
						</p>
					</div>
					<div className="flex flex-col gap-1 p-3">
						<Shortcut
							to="/customers"
							icon={<Users className="size-4" />}
							label="Gérer les clients"
						/>
						<Shortcut
							to="/customers"
							icon={<FileText className="size-4" />}
							label="Créer un devis"
							disabled
						/>
						<Shortcut
							to="/customers"
							icon={<Receipt className="size-4" />}
							label="Nouvelle facture"
							disabled
						/>
					</div>
				</div>
			</section>
		</div>
	)
}

interface StatCardProps {
	label: string
	value: string
	hint: string
	icon: React.ReactNode
}

function StatCard({ label, value, hint, icon }: StatCardProps) {
	return (
		<div className="flex flex-col gap-4 rounded-2xl border bg-card p-5">
			<div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
				<span className="flex size-7 items-center justify-center rounded-lg bg-muted">
					{icon}
				</span>
				{label}
			</div>
			<p className="text-4xl font-bold tracking-tight">{value}</p>
			<p className="text-xs text-muted-foreground">{hint}</p>
		</div>
	)
}

interface ActivityRowProps {
	letter: string
	color: string
	title: string
	badge: string
	badgeColor: string
	subtitle: string
	meta: string
}

function ActivityRow({
	letter,
	color,
	title,
	badge,
	badgeColor,
	subtitle,
	meta,
}: ActivityRowProps) {
	return (
		<li className="flex items-center gap-3 px-5 py-3">
			<div
				className={`flex size-10 shrink-0 items-center justify-center rounded-lg text-sm font-semibold text-white ${color}`}
			>
				{letter}
			</div>
			<div className="min-w-0 flex-1">
				<div className="flex items-center gap-2">
					<p className="truncate font-medium">{title}</p>
					<span
						className={`inline-flex shrink-0 items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${badgeColor}`}
					>
						{badge}
					</span>
				</div>
				<p className="truncate text-xs text-muted-foreground">{subtitle}</p>
			</div>
			<span className="text-xs text-muted-foreground">{meta}</span>
		</li>
	)
}

interface ShortcutProps {
	to: string
	icon: React.ReactNode
	label: string
	disabled?: boolean
}

function Shortcut({ to, icon, label, disabled }: ShortcutProps) {
	if (disabled) {
		return (
			<div className="flex cursor-not-allowed items-center gap-3 rounded-lg px-3 py-2 text-sm text-muted-foreground opacity-60">
				{icon}
				<span className="flex-1">{label}</span>
				<span className="rounded-md bg-muted px-1.5 py-0.5 text-[10px] font-medium">
					soon
				</span>
			</div>
		)
	}
	return (
		<Link
			to={to}
			className="flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors hover:bg-muted"
		>
			{icon}
			<span className="flex-1">{label}</span>
			<ArrowRight className="size-4 opacity-60" />
		</Link>
	)
}
