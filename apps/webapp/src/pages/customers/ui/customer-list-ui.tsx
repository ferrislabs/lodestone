import { MoreHorizontal, Plus, Search, Trash2, UserPlus } from 'lucide-react'
import { useMemo } from 'react'
import { Button } from '#/components/ui/button'
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from '#/components/ui/dropdown-menu'
import {
	CATEGORY_LABELS,
	type Customer,
	type CustomerCategory,
} from '#/pages/customers/types'

interface CustomerListUIProps {
	customers: Customer[]
	/// Search and filter are owned by the feature, per the project's
	/// feature/UI split: when the list becomes server-paginated this state has
	/// to travel with the query, and it cannot do that from down here.
	search: string
	filter: Filter
	onSearchChange: (search: string) => void
	onFilterChange: (filter: Filter) => void
	isLoading?: boolean
	onAdd?: () => void
	onEdit?: (customer: Customer) => void
	onDelete?: (customer: Customer) => void
}

const AVATAR_COLOR: Record<CustomerCategory, string> = {
	artisan: 'bg-amber-500',
	sme: 'bg-blue-500',
	individual: 'bg-emerald-500',
}

const BADGE_COLOR: Record<CustomerCategory, string> = {
	artisan:
		'bg-amber-50 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300',
	sme: 'bg-blue-50 text-blue-700 dark:bg-blue-500/15 dark:text-blue-300',
	individual:
		'bg-emerald-50 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300',
}

export type Filter = 'all' | CustomerCategory

const FILTERS: { id: Filter; label: string }[] = [
	{ id: 'all', label: 'Tous' },
	{ id: 'artisan', label: 'Artisans' },
	{ id: 'sme', label: 'PME' },
	{ id: 'individual', label: 'Particuliers' },
]

export function CustomerListUI({
	customers,
	search,
	filter,
	onSearchChange,
	onFilterChange,
	isLoading,
	onAdd,
	onEdit,
	onDelete,
}: CustomerListUIProps) {
	const counts = useMemo(() => {
		const c = {
			total: customers.length,
			artisan: 0,
			sme: 0,
			individual: 0,
		}
		for (const x of customers) c[x.category]++
		return c
	}, [customers])

	const visible = useMemo(() => {
		const q = search.trim().toLowerCase()
		return customers.filter((c) => {
			if (filter !== 'all' && c.category !== filter) return false
			if (!q) return true
			return (
				c.name.toLowerCase().includes(q) ||
				c.contact_name.toLowerCase().includes(q) ||
				c.email.toLowerCase().includes(q) ||
				c.address.city.toLowerCase().includes(q)
			)
		})
	}, [customers, search, filter])

	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<header className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
				<div>
					<h1 className="text-2xl font-bold tracking-tight md:text-[28px]">
						Fichier client
					</h1>
					<p className="mt-1 text-sm text-muted-foreground">
						Gérez vos clients, leurs contacts et leurs informations
						commerciales.
					</p>
				</div>
				<Button
					onClick={onAdd}
					className="h-10 rounded-xl bg-orange-600 px-4 font-semibold text-white hover:bg-orange-700"
				>
					<Plus />
					Nouveau client
				</Button>
			</header>

			<div className="flex flex-wrap gap-2">
				{FILTERS.map((f) => {
					const active = f.id === filter
					return (
						<button
							type="button"
							key={f.id}
							onClick={() => onFilterChange(f.id)}
							className={`rounded-xl border px-4 py-1.5 text-sm font-medium transition-colors ${
								active
									? 'border-orange-200 bg-orange-50 text-orange-700 dark:border-orange-500/30 dark:bg-orange-500/15 dark:text-orange-300'
									: 'border-border bg-card text-muted-foreground hover:bg-muted'
							}`}
						>
							{f.label}
						</button>
					)
				})}
			</div>

			<section>
				<p className="mb-3 text-sm text-muted-foreground">Aperçu du fichier</p>
				<div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
					<MiniStat
						label="Total clients"
						value={counts.total}
						hint="Tous clients confondus"
					/>
					<MiniStat
						label="Artisans"
						value={counts.artisan}
						hint="Clients professionnels"
					/>
					<MiniStat
						label="PME"
						value={counts.sme}
						hint="Petites et moyennes entreprises"
					/>
					<MiniStat
						label="Particuliers"
						value={counts.individual}
						hint="Clients privés"
					/>
				</div>
			</section>

			<section className="flex flex-col gap-3">
				<div className="flex flex-col items-start justify-between gap-3 sm:flex-row sm:items-center">
					<h2 className="font-semibold">Clients ({visible.length})</h2>
					<div className="relative w-full sm:w-72">
						<Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
						<input
							type="search"
							value={search}
							onChange={(e) => onSearchChange(e.target.value)}
							placeholder="Rechercher un client…"
							className="h-10 w-full rounded-xl border bg-card pl-9 pr-3 text-sm outline-none transition-colors placeholder:text-muted-foreground focus:border-orange-400 focus:ring-2 focus:ring-orange-100 dark:focus:ring-orange-500/20"
						/>
					</div>
				</div>

				{isLoading ? (
					<div className="flex items-center justify-center rounded-2xl border bg-card p-12 text-sm text-muted-foreground">
						Chargement…
					</div>
				) : visible.length === 0 ? (
					<div className="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed bg-card p-12 text-center">
						<div className="flex size-12 items-center justify-center rounded-xl bg-muted">
							<UserPlus className="size-6 text-muted-foreground" />
						</div>
						<div>
							<p className="font-medium">Aucun client trouvé</p>
							<p className="text-sm text-muted-foreground">
								{search || filter !== 'all'
									? "Essayez d'autres critères"
									: 'Commencez par ajouter votre premier client'}
							</p>
						</div>
						{!search && filter === 'all' && (
							<Button onClick={onAdd} variant="outline" className="rounded-xl">
								<Plus />
								Ajouter un client
							</Button>
						)}
					</div>
				) : (
					<ul className="overflow-hidden rounded-2xl border bg-card divide-y">
						{visible.map((c) => (
							<li
								key={c.id}
								className="flex items-center gap-4 px-5 py-4 transition-colors hover:bg-muted/40"
							>
								<div
									className={`flex size-11 shrink-0 items-center justify-center rounded-xl text-base font-bold text-white ${
										AVATAR_COLOR[c.category]
									}`}
								>
									{c.name[0]?.toUpperCase()}
								</div>

								<div className="min-w-0 flex-1">
									<div className="flex items-center gap-2">
										<p className="truncate font-semibold">{c.name}</p>
										<span
											className={`inline-flex shrink-0 items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${
												BADGE_COLOR[c.category]
											}`}
										>
											{CATEGORY_LABELS[c.category].toLowerCase()}
										</span>
									</div>
									<p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
										id: {c.id}
									</p>
								</div>

								<div className="hidden flex-col items-end gap-0.5 text-xs text-muted-foreground md:flex">
									<span className="truncate">{c.email}</span>
									<span>{c.address.city}</span>
								</div>

								<span className="hidden items-center rounded-md border bg-card px-2 py-1 text-[11px] font-medium text-muted-foreground lg:inline-flex">
									{c.phone}
								</span>

								<DropdownMenu>
									<DropdownMenuTrigger asChild>
										<Button
											variant="ghost"
											size="icon-sm"
											className="rounded-lg text-muted-foreground"
										>
											<MoreHorizontal />
											<span className="sr-only">Actions</span>
										</Button>
									</DropdownMenuTrigger>
									<DropdownMenuContent align="end">
										<DropdownMenuItem onClick={() => onEdit?.(c)}>
											Modifier
										</DropdownMenuItem>
										<DropdownMenuSeparator />
										<DropdownMenuItem
											variant="destructive"
											onClick={() => onDelete?.(c)}
										>
											<Trash2 />
											Supprimer
										</DropdownMenuItem>
									</DropdownMenuContent>
								</DropdownMenu>
							</li>
						))}
					</ul>
				)}
			</section>
		</div>
	)
}

interface MiniStatProps {
	label: string
	value: number
	hint: string
}

function MiniStat({ label, value, hint }: MiniStatProps) {
	return (
		<div className="flex flex-col gap-3 rounded-2xl border bg-card p-5">
			<p className="text-sm font-medium text-muted-foreground">{label}</p>
			<p className="text-3xl font-bold tracking-tight">{value}</p>
			<p className="text-xs text-muted-foreground">{hint}</p>
		</div>
	)
}
