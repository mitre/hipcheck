/**
 * A bounded, render-safe error returned by the frontend API boundary.
 *
 * API response bodies and network errors are deliberately not passed to UI
 * callers: they can contain arbitrary server or upstream text. Routes may
 * log the original failure using their normal server-side logging policy.
 */
export type ApiError = {
	kind: 'network' | 'request' | 'not-found' | 'conflict' | 'rate-limited' | 'server' | 'unexpected';
	message: string;
	retryable: boolean;
	status?: number;
};

type ClientFailure = { error?: unknown; request?: Request; response?: Response };

const unexpectedError = (): ApiError => ({
	kind: 'unexpected',
	message: 'The request could not be completed.',
	retryable: false
});

const statusError = (status: number): ApiError => {
	if (status === 400 || status === 422) {
		return { kind: 'request', message: 'The request could not be processed.', retryable: false, status };
	}
	if (status === 404) {
		return { kind: 'not-found', message: 'The requested resource was not found.', retryable: false, status };
	}
	if (status === 409) {
		return { kind: 'conflict', message: 'The request conflicts with the current resource state.', retryable: true, status };
	}
	if (status === 429) {
		return { kind: 'rate-limited', message: 'Too many requests were made. Please try again shortly.', retryable: true, status };
	}
	if (status >= 500) {
		return { kind: 'server', message: 'Night Vision is temporarily unavailable. Please try again.', retryable: true, status };
	}

	return { kind: 'unexpected', message: 'The request could not be completed.', retryable: false, status };
};

/** Convert generated-client failures into a stable UI contract. */
export const normalizeApiError = ({ error, request, response }: ClientFailure): ApiError => {
	if (response && !response.ok) return statusError(response.status);

	if (response) {
		// A successful HTTP status can still carry malformed JSON. Other failures
		// while handling a response may come from client-side processing.
		return error instanceof SyntaxError
			? {
					kind: 'server',
					message: 'Night Vision returned an invalid response. Please try again.',
					retryable: true,
					status: response.status
				}
			: { ...unexpectedError(), status: response.status };
	}

	if (request && (error instanceof TypeError || (error instanceof DOMException && error.name === 'NetworkError'))) {
		return {
			kind: 'network',
			message: 'Unable to reach Night Vision. Check your connection and try again.',
			retryable: true
		};
	}

	return unexpectedError();
};
