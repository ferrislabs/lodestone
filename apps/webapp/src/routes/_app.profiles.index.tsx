import { createFileRoute } from '@tanstack/react-router'

import { ProfilesListFeature } from '#/pages/profiles/feature/profiles-list-feature'

export const Route = createFileRoute('/_app/profiles/')({
	component: ProfilesListFeature,
})
