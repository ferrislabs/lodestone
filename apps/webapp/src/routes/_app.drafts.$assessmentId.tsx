import { createFileRoute } from '@tanstack/react-router'

import { DraftReviewFeature } from '#/pages/drafts/feature/draft-review-feature'

export const Route = createFileRoute('/_app/drafts/$assessmentId')({
	component: DraftReviewFeature,
})
