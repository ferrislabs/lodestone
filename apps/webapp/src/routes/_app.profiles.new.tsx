import { createFileRoute } from '@tanstack/react-router'

import { ProfileNewFeature } from '#/pages/profiles/feature/profile-new-feature'

export const Route = createFileRoute('/_app/profiles/new')({
	component: ProfileNewFeature,
})
