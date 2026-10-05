import { flexRender, type Table as TanstackTable } from '@tanstack/react-table'
import { Download } from 'lucide-react'
import type { ReactNode } from 'react'

import { Button } from '#/components/ui/button'
import { Skeleton } from '#/components/ui/skeleton'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import type { Lead, PaginationMeta } from '#/pages/leads/lib/types'

export type ExportFormat = 'json' | 'csv'

const SKELETON_ROWS = ['a', 'b', 'c', 'd', 'e', 'f']

interface LeadsListProps {
	toolbar: ReactNode
	table: TanstackTable<Lead>
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	pagination: PaginationMeta | null
	onPageChange: (page: number) => void
	onExport: (format: ExportFormat) => void
	exporting: boolean
	exportError: string | null
	columnCount: number
	noProfile: boolean
}

export function LeadsList({
	toolbar,
	table,
	state,
	errorMessage,
	pagination,
	onPageChange,
	onExport,
	exporting,
	exportError,
	columnCount,
	noProfile,
}: LeadsListProps) {
	const rows = table.getRowModel().rows
	return (
		<div className="flex flex-col gap-4 p-4 md:p-8">
			<div className="flex flex-wrap items-center justify-between gap-2">
				<h1 className="text-2xl font-bold tracking-tight">Leads</h1>
				<div className="flex gap-2">
					<Button
						variant="outline"
						size="sm"
						disabled={exporting || noProfile}
						onClick={() => onExport('json')}
					>
						<Download />
						Export JSON
					</Button>
					<Button
						variant="outline"
						size="sm"
						disabled={exporting || noProfile}
						onClick={() => onExport('csv')}
					>
						<Download />
						Export CSV
					</Button>
				</div>
			</div>
			{exportError ? (
				<p role="alert" className="text-sm text-destructive">
					Export failed: {exportError}
				</p>
			) : null}
			{toolbar}
			<div className="rounded-xl border bg-card">
				<Table>
					<TableHeader>
						{table.getHeaderGroups().map((group) => (
							<TableRow key={group.id}>
								{group.headers.map((header) => (
									<TableHead key={header.id}>
										{flexRender(
											header.column.columnDef.header,
											header.getContext(),
										)}
									</TableHead>
								))}
							</TableRow>
						))}
					</TableHeader>
					<TableBody>
						{state === 'loading'
							? SKELETON_ROWS.map((id) => (
									<TableRow key={id}>
										<TableCell colSpan={columnCount}>
											<Skeleton className="h-5 w-full" />
										</TableCell>
									</TableRow>
								))
							: null}
						{state === 'error' ? (
							<TableRow>
								<TableCell
									colSpan={columnCount}
									className="py-10 text-center text-destructive"
									role="alert"
								>
									Could not load leads: {errorMessage}
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready' && rows.length === 0 ? (
							<TableRow>
								<TableCell
									colSpan={columnCount}
									className="py-10 text-center text-muted-foreground"
								>
									{noProfile
										? 'No detection profile yet.'
										: 'No leads match these filters.'}
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready'
							? rows.map((row) => (
									<TableRow key={row.id}>
										{row.getVisibleCells().map((cell) => (
											<TableCell key={cell.id}>
												{flexRender(
													cell.column.columnDef.cell,
													cell.getContext(),
												)}
											</TableCell>
										))}
									</TableRow>
								))
							: null}
					</TableBody>
				</Table>
			</div>
			{pagination && state === 'ready' ? (
				<div className="flex items-center justify-between text-sm text-muted-foreground">
					<span>
						Page {pagination.current_page}
						{pagination.last_page ? ` of ${pagination.last_page}` : ''}
						{pagination.total === null || pagination.total === undefined
							? ''
							: ` - ${pagination.total} leads`}
					</span>
					<div className="flex gap-2">
						<Button
							variant="outline"
							size="sm"
							disabled={!pagination.prev_page}
							onClick={() => onPageChange(pagination.prev_page ?? 1)}
						>
							Previous
						</Button>
						<Button
							variant="outline"
							size="sm"
							disabled={!pagination.next_page}
							onClick={() =>
								onPageChange(pagination.next_page ?? pagination.current_page)
							}
						>
							Next
						</Button>
					</div>
				</div>
			) : null}
		</div>
	)
}
