import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'

describe('PROFILE_SCHEMA', () => {
	it('equals the committed docs schema', () => {
		const committed: unknown = JSON.parse(
			readFileSync(
				resolve(process.cwd(), '../../docs/schemas/profile.v10.schema.json'),
				'utf8',
			),
		)
		expect(PROFILE_SCHEMA).toEqual(committed)
	})
})
