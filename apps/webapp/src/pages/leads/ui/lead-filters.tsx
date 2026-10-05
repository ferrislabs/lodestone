import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import {
	filtersFromForm,
	LEAD_SORTS,
	type LeadFilters,
} from '#/pages/leads/lib/search'
import { LEAD_STATUSES } from '#/pages/leads/lib/transitions'
import type { ProfileSummary } from '#/pages/leads/lib/types'

const SORT_LABELS: Record<(typeof LEAD_SORTS)[number], string> = {
	score_desc: 'Score, highest first',
	score_asc: 'Score, lowest first',
	last_seen_desc: 'Last seen, newest first',
	first_seen_desc: 'First seen, newest first',
	domain_asc: 'Domain, A to Z',
}

const SELECT_CLASS =
	'h-9 w-full rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'

interface LeadFiltersFormProps {
	profiles: readonly ProfileSummary[]
	profileId: string | undefined
	filters: LeadFilters
	onProfileChange: (profileId: string) => void
	onApply: (filters: LeadFilters) => void
	onReset: () => void
}

export function LeadFiltersForm({
	profiles,
	profileId,
	filters,
	onProfileChange,
	onApply,
	onReset,
}: LeadFiltersFormProps) {
	return (
		<div className="flex flex-col gap-4">
			<div className="flex max-w-sm flex-col gap-1.5">
				<Label htmlFor="profile">Profile</Label>
				<select
					id="profile"
					className={SELECT_CLASS}
					value={profileId ?? ''}
					onChange={(event) => onProfileChange(event.target.value)}
				>
					{profiles.map((profile) => (
						<option key={profile.id} value={profile.id}>
							{profile.name} ({profile.lead_count})
							{profile.active ? '' : ' - inactive'}
						</option>
					))}
				</select>
			</div>
			<form
				key={JSON.stringify(filters)}
				className="grid gap-3 md:grid-cols-3 lg:grid-cols-6"
				onSubmit={(event) => {
					event.preventDefault()
					onApply(filtersFromForm(new FormData(event.currentTarget)))
				}}
			>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="status">Status</Label>
					<select
						id="status"
						name="status"
						className={SELECT_CLASS}
						defaultValue={filters.status}
					>
						<option value="">Any</option>
						{LEAD_STATUSES.map((status) => (
							<option key={status} value={status}>
								{status}
							</option>
						))}
					</select>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="min_score">Min score</Label>
					<Input
						id="min_score"
						name="min_score"
						type="number"
						min={0}
						step="any"
						defaultValue={filters.minScore}
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="domain">Domain contains</Label>
					<Input id="domain" name="domain" defaultValue={filters.domain} />
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="signal">Signal kind</Label>
					<Input
						id="signal"
						name="signal"
						placeholder="e.g. hiring_page"
						defaultValue={filters.signal}
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="sort">Sort</Label>
					<select
						id="sort"
						name="sort"
						className={SELECT_CLASS}
						defaultValue={filters.sort}
					>
						{LEAD_SORTS.map((sort) => (
							<option key={sort} value={sort}>
								{SORT_LABELS[sort]}
							</option>
						))}
					</select>
				</div>
				<div className="flex items-end gap-2">
					<Button type="submit">Apply</Button>
					<Button type="button" variant="outline" onClick={onReset}>
						Reset
					</Button>
				</div>
			</form>
		</div>
	)
}
