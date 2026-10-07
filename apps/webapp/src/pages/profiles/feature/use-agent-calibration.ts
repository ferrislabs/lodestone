import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'

import { lodestoneApi } from '#/api/lodestone'
import {
	type CalibrationStates,
	calibrationStates,
} from '#/pages/profiles/lib/agent-calibration'

export function useAgentCalibration(
	profileId: string | null,
): CalibrationStates {
	const query = useQuery({
		...lodestoneApi().get('/api/v1/quality/calibration').queryOptions,
		enabled: profileId !== null,
	})
	const data = query.data?.data
	return useMemo(() => calibrationStates(data, profileId), [data, profileId])
}
