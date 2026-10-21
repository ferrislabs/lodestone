import { toSchema } from '#/lib/schema-form'
import schemaJson from '#/pages/profiles/lib/profile.v15.schema.json'

export const PROFILE_SCHEMA = toSchema(schemaJson)
