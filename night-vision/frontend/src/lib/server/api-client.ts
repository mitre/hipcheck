import { env } from '$env/dynamic/private';
import { createClient } from '$lib/api/generated/client';

const LOCAL_API_BASE_URL = 'http://127.0.0.1:8080';

/**
 * The sole server-side configuration path for the generated API client.
 * Routes must use helpers from `$lib/server/night-vision-api` instead of
 * configuring a generated client themselves.
 */
export const getApiBaseUrl = (configuredUrl = env.API_BASE_URL): string => {
	const candidate = configuredUrl ?? LOCAL_API_BASE_URL;
	let parsed: URL;

	try {
		parsed = new URL(candidate);
	} catch {
		throw new Error('API_BASE_URL must be an absolute HTTP(S) URL.');
	}

	if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
		throw new Error('API_BASE_URL must use HTTP or HTTPS.');
	}

	return parsed.toString().replace(/\/$/, '');
};

export const nightVisionClient = createClient({ baseUrl: getApiBaseUrl() });
