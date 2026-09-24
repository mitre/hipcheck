import type { ApiError } from '$lib/api/errors';

// SvelteKit's error() only accepts 400-599. Network failures carry no status
// and an unreadable success response carries a 2xx one, so both become 502.
export const pageErrorStatus = ({ status }: ApiError): number =>
	status !== undefined && status >= 400 && status <= 599 ? status : 502;
