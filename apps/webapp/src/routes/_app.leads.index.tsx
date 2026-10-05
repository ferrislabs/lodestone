import { createFileRoute } from '@tanstack/react-router'

import { LeadsListFeature } from '#/pages/leads/feature/leads-list-feature'
import { leadsSearchSchema } from '#/pages/leads/lib/search'

export const Route = createFileRoute('/_app/leads/')({
	validateSearch: leadsSearchSchema,
	head: () => ({ meta: [{ title: 'Leads · Lodestone' }] }),
	component: LeadsListFeature,
})
