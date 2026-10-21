import { createFileRoute } from '@tanstack/react-router'
import { ProfileDetailFeature } from '#/pages/profiles/feature/profile-detail-feature'
import { tabSearchSchema } from '#/pages/profiles/lib/tabs'

export const Route = createFileRoute('/_app/profiles/$profileId')({
	validateSearch: tabSearchSchema,
	component: ProfileDetailFeature,
})
