import { describe, expect, it } from 'vitest'

import {
	appendIn,
	classify,
	defaultValue,
	enabledValue,
	fieldPaths,
	formatPath,
	genericPath,
	getIn,
	humanize,
	type JsonValue,
	propertiesOf,
	removeIn,
	resolve,
	type Schema,
	setIn,
	validateValue,
	variantIndex,
} from '#/lib/schema-form'
import { loadProfileSchema } from '#/lib/schema-form/test-schema'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'

const root = loadProfileSchema()

function def(name: string): Schema {
	const found = root.$defs?.[name]
	if (!found) throw new Error(`missing def ${name}`)
	return found
}

describe('resolve', () => {
	it('follows $ref and lets siblings override', () => {
		const resolved = resolve(root, {
			$ref: '#/$defs/Identity',
			description: 'Custom',
		})
		expect(resolved.title).toBe('Identity')
		expect(resolved.description).toBe('Custom')
		expect(resolved.properties).toHaveProperty('offer')
	})

	it('leaves unknown refs untouched', () => {
		expect(resolve(root, { $ref: '#/$defs/Nope' }).$ref).toBe('#/$defs/Nope')
	})
})

describe('classify', () => {
	const unions = ['Matcher', 'VersionExtractor', 'DebtRule', 'AgentSource']

	it('classifies every $def of the committed schema', () => {
		const names = Object.keys(root.$defs ?? {})
		expect(names.length).toBe(16)
		for (const name of names) {
			const node = classify(root, { $ref: `#/$defs/${name}` })
			const expected = unions.includes(name)
				? 'union'
				: name === 'AgentId'
					? 'string'
					: name === 'Language'
						? 'enum'
						: 'object'
			expect(node.kind, name).toBe(expected)
		}
	})

	it('never leaves a property unclassified', () => {
		const visit = (schema: Schema, trail: string) => {
			const node = classify(root, schema)
			expect(node.kind, trail).not.toBe('unknown')
			if (node.kind === 'object') {
				for (const property of node.properties) {
					visit(property.schema, `${trail}.${property.key}`)
				}
			}
			if (node.kind === 'array') visit(node.items, `${trail}[]`)
			if (node.kind === 'nullable') visit(node.inner, trail)
			if (node.kind === 'union') {
				for (const variant of node.variants) visit(variant.schema, trail)
			}
		}
		visit(root, '$')
	})

	it('reads tagged unions through the const discriminator', () => {
		const node = classify(root, { $ref: '#/$defs/Matcher' })
		if (node.kind !== 'union') throw new Error('not a union')
		expect(node.discriminator).toBe('kind')
		expect(node.variants.map((variant) => variant.value)).toEqual([
			'status',
			'header',
			'body',
		])
	})

	it('treats anyOf null and type arrays as nullable', () => {
		const llm = def('Agent').properties?.llm ?? {}
		expect(classify(root, llm).kind).toBe('nullable')
		const value = def('ScoringFactor').properties?.value ?? {}
		const node = classify(root, value)
		expect(node.kind).toBe('nullable')
		if (node.kind === 'nullable') expect(node.inner.type).toBe('string')
	})

	it('classifies numbers with their format', () => {
		const equals = def('Matcher').oneOf?.[0]?.properties?.equals ?? {}
		const node = classify(root, equals)
		expect(node.kind).toBe('number')
		if (node.kind === 'number') {
			expect(node.integer).toBe(true)
			expect(node.schema.maximum).toBe(65535)
		}
		const weight = def('ScoringFactor').properties?.weight ?? {}
		const weightNode = classify(root, weight)
		if (weightNode.kind === 'number') {
			expect(weightNode.integer).toBe(true)
			expect(weightNode.schema.minimum).toBe(1)
			expect(weightNode.schema.maximum).toBe(10)
		}
	})

	it('orders required properties first', () => {
		expect(propertiesOf(def('Fingerprint')).map((p) => p.key)).toEqual([
			'id',
			'path',
			'product',
			'matchers',
			'debt_markers',
			'version',
		])
	})
})

describe('defaultValue', () => {
	it('builds a new profile with every required key', () => {
		const value = defaultValue(root, root, PROFILE_HINTS)
		expect(value).toEqual({
			schema_version: 3,
			identity: { offer: '', target_customer: '' },
			agents: [],
			scoring: { factors: [] },
			contact_ranking: { preferred_local_parts: [], guess_local_parts: [] },
			llm: {
				qualification_threshold: 50,
				relevance_criteria: '',
				language: 'fr',
			},
		})
		for (const key of root.required ?? []) expect(value).toHaveProperty(key)
	})

	it('fills schema defaults for keyword sources', () => {
		const source = defaultValue(
			root,
			{ $ref: '#/$defs/AgentSource' },
			PROFILE_HINTS,
		)
		expect(source).toEqual({
			kind: 'certificate_transparency',
			labels: [],
		})
		const boamp = def('AgentSource').oneOf?.[3] ?? {}
		expect(defaultValue(root, boamp, PROFILE_HINTS)).toEqual({
			kind: 'boamp',
			keywords: [],
			exclusions: [],
			accept_min_keywords: 2,
		})
	})

	it('starts unions on their first variant with the discriminator set', () => {
		expect(
			defaultValue(root, { $ref: '#/$defs/Matcher' }, PROFILE_HINTS, [
				'agents',
				0,
				'source',
				'fingerprints',
				0,
				'matchers',
				0,
			]),
		).toEqual({ kind: 'status', equals: 200 })
		expect(
			defaultValue(root, { $ref: '#/$defs/DebtRule' }, PROFILE_HINTS),
		).toEqual({ kind: 'matches', matcher: { kind: 'status', equals: 0 } })
	})

	it('gives an optional section its enabled value', () => {
		const llm = def('Agent').properties?.llm ?? {}
		expect(
			enabledValue(root, llm, PROFILE_HINTS, ['agents', 0, 'llm']),
		).toEqual({})
	})
})

describe('paths', () => {
	it('formats the same paths as the server', () => {
		expect(formatPath([])).toBe('$')
		expect(formatPath(['identity', 'offer'])).toBe('identity.offer')
		expect(formatPath(['scoring', 'factors', 0, 'weight'])).toBe(
			'scoring.factors[0].weight',
		)
		expect(
			formatPath([
				'agents',
				0,
				'source',
				'fingerprints',
				0,
				'matchers',
				1,
				'pattern',
			]),
		).toBe('agents[0].source.fingerprints[0].matchers[1].pattern')
		expect(formatPath(['agents', 1, 'source', 'labels', 1])).toBe(
			'agents[1].source.labels[1]',
		)
		expect(formatPath(['contact_ranking', 'preferred_local_parts', 1])).toBe(
			'contact_ranking.preferred_local_parts[1]',
		)
	})

	it('drops indexes for hint lookups', () => {
		expect(genericPath(['scoring', 'factors', 2, 'weight'])).toBe(
			'scoring.factors.weight',
		)
	})

	it('humanizes property names', () => {
		expect(humanize('target_customer')).toBe('Target customer')
	})
})

describe('immutable ops', () => {
	const original: JsonValue = {
		scoring: { factors: [{ signal: 'a', weight: 1 }] },
		sources: { boamp: { keywords: [] } },
	}

	it('sets without mutating', () => {
		const next = setIn(original, ['scoring', 'factors', 0, 'weight'], 5)
		expect(getIn(next, ['scoring', 'factors', 0, 'weight'])).toBe(5)
		expect(getIn(original, ['scoring', 'factors', 0, 'weight'])).toBe(1)
		expect(getIn(next, ['sources'])).toBe(getIn(original, ['sources']))
	})

	it('removes keys and array rows', () => {
		const withoutBoamp = removeIn(original, ['sources', 'boamp'])
		expect(getIn(withoutBoamp, ['sources'])).toEqual({})
		expect(getIn(original, ['sources', 'boamp'])).toBeDefined()
		const two = appendIn(original, ['scoring', 'factors'], {
			signal: 'b',
			weight: 2,
		})
		expect(getIn(two, ['scoring', 'factors'])).toHaveLength(2)
		const one = removeIn(two, ['scoring', 'factors', 0])
		expect(getIn(one, ['scoring', 'factors', 0, 'signal'])).toBe('b')
		expect(getIn(original, ['scoring', 'factors'])).toHaveLength(1)
	})

	it('creates missing containers when setting', () => {
		expect(setIn({}, ['a', 'b', 1], 'x')).toEqual({
			a: { b: [undefined, 'x'] },
		})
	})
})

describe('validateValue', () => {
	it('flags empty required text and out of range numbers', () => {
		const value = defaultValue(root, root, PROFILE_HINTS)
		const problems = validateValue(root, root, value)
		expect(problems).toContainEqual({
			path: 'identity.offer',
			message: 'must not be empty',
		})
		const bad = setIn(
			setIn(value, ['scoring', 'factors'], [{ signal: 's', weight: null }]),
			['agents'],
			[
				{
					id: 'x',
					name: 'n',
					enabled: true,
					source: {
						kind: 'http_probe',
						fingerprints: [
							{
								id: 'x',
								path: '/',
								product: 'p',
								matchers: [{ kind: 'status', equals: 70000 }],
							},
						],
					},
				},
			],
		)
		const paths = validateValue(root, root, bad).map((p) => p.path)
		expect(paths).toContain('scoring.factors[0].weight')
		expect(paths).toContain(
			'agents[0].source.fingerprints[0].matchers[0].equals',
		)
	})

	it('reports a missing required key', () => {
		const problems = validateValue(root, root, { schema_version: 3 })
		expect(problems.map((p) => p.path)).toContain('identity')
	})
})

describe('fieldPaths', () => {
	it('covers rendered fields so problems map onto them', () => {
		const value = defaultValue(root, root, PROFILE_HINTS)
		const withFactor = setIn(
			value,
			['scoring', 'factors'],
			[{ signal: 's', weight: 1 }],
		)
		const paths = fieldPaths(root, root, withFactor)
		expect(paths.has('scoring.factors[0].weight')).toBe(true)
		expect(paths.has('scoring.factors[0].value')).toBe(true)
		expect(paths.has('identity.offer')).toBe(true)
		expect(paths.has('agents')).toBe(true)
		expect(paths.has('$')).toBe(true)
		expect(paths.has('bogus')).toBe(false)
	})
})

describe('variantIndex', () => {
	it('selects the variant matching the discriminator', () => {
		const node = classify(root, { $ref: '#/$defs/Matcher' })
		if (node.kind !== 'union') throw new Error('not a union')
		expect(variantIndex(node.variants, { kind: 'body', pattern: '' })).toBe(2)
		expect(variantIndex(node.variants, undefined)).toBe(0)
	})
})
