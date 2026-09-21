import type {
	GetPackageSourceErrors,
	GetPackageSourceResponse,
	GetPackageSourceResponses,
	GetUpgradeAssessmentErrors,
	GetUpgradeAssessmentResponse,
	GetUpgradeAssessmentResponses,
	PostPackageSourceBody,
	PostPackageSourceErrors,
	PostPackageSourceResponse,
	PostPackageSourceResponses,
	PostUpgradeAssessmentData,
	PostUpgradeAssessmentErrors,
	PostUpgradeAssessmentResponse,
	PostUpgradeAssessmentResponses
} from '$lib/api/generated';
import type { Client } from '$lib/api/generated/client/types.gen';
import { normalizeApiError, type ApiError } from '$lib/api/errors';
import { nightVisionClient } from './api-client';

export type ApiResult<T> = { ok: true; data: T } | { ok: false; error: ApiError };

type ClientResult<T> = { data?: T; error?: unknown; response?: Response };

const toApiResult = <T>({ data, error, response }: ClientResult<T>): ApiResult<T> => {
	if (error !== undefined || data === undefined) return { ok: false, error: normalizeApiError(response) };

	return { ok: true, data };
};

/** Typed helpers are the only supported route-facing API boundary. */
export const createNightVisionApi = (client: Client = nightVisionClient) => ({
	submitPackageSource: async (body: PostPackageSourceBody): Promise<ApiResult<PostPackageSourceResponse>> =>
		toApiResult(
			await client.post<PostPackageSourceResponses, PostPackageSourceErrors>({
				url: '/package-sources',
				body
			})
		),

	getPackageSourceStatus: async (id: string): Promise<ApiResult<GetPackageSourceResponse>> =>
		toApiResult(
			await client.get<GetPackageSourceResponses, GetPackageSourceErrors>({
				url: '/package-sources/{id}',
				path: { id }
			})
		),

	submitUpgradeAssessment: async (
		body: PostUpgradeAssessmentData['body']
	): Promise<ApiResult<PostUpgradeAssessmentResponse>> =>
		toApiResult(
			await client.post<PostUpgradeAssessmentResponses, PostUpgradeAssessmentErrors>({
				url: '/upgrade-assessments',
				body
			})
		),

	getUpgradeAssessmentStatus: async (id: string): Promise<ApiResult<GetUpgradeAssessmentResponse>> =>
		toApiResult(
			await client.get<GetUpgradeAssessmentResponses, GetUpgradeAssessmentErrors>({
				url: '/upgrade-assessments/{id}',
				path: { id }
			})
		)
});

export const nightVisionApi = createNightVisionApi();

export const isPackageSourceTerminal = (status: GetPackageSourceResponse): boolean =>
	status.status !== 'processing';

export const isUpgradeAssessmentTerminal = (status: GetUpgradeAssessmentResponse): boolean =>
	status.status !== 'pending';
