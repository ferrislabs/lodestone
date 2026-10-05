import { createFileRoute } from '@tanstack/react-router'

import { ProfileDetailFeature } from '#/pages/profiles/feature/profile-detail-feature'

export const Route = createFileRoute('/_app/profiles/$profileId')({
	component: ProfileDetailFeature,
})
