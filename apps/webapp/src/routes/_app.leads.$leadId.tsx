import { createFileRoute } from '@tanstack/react-router'

import { LeadDetailFeature } from '#/pages/leads/feature/lead-detail-feature'

export const Route = createFileRoute('/_app/leads/$leadId')({
	component: LeadDetailFeature,
})
