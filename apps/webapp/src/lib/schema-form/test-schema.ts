import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { type Schema, toSchema } from '#/lib/schema-form/types'

const SCHEMA_PATH = resolve(
	process.cwd(),
	'../../docs/schemas/profile.v4.schema.json',
)

export function loadProfileSchema(): Schema {
	return toSchema(JSON.parse(readFileSync(SCHEMA_PATH, 'utf8')))
}
