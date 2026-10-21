import { toSchema } from '#/lib/schema-form'
import schemaJson from '#/pages/profiles/lib/profile.v8.schema.json'

export const PROFILE_SCHEMA = toSchema(schemaJson)
