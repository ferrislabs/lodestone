import { createFileRoute } from '@tanstack/react-router'
import { ProfileNewFeature } from '#/pages/profiles/feature/profile-new-feature'
import { tabSearchSchema } from '#/pages/profiles/lib/tabs'

export const Route = createFileRoute('/_app/profiles/new')({
	validateSearch: tabSearchSchema,
	head: () => ({ meta: [{ title: 'Nouveau profil · Lodestone' }] }),
	component: ProfileNewFeature,
})
