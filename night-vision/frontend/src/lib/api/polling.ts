/** A terminal result from a bounded asynchronous-status poll. */
export type PollResult<T> =
	| { kind: 'completed'; value: T; attempts: number }
	| { kind: 'cancelled'; attempts: number }
	| { kind: 'timed-out'; attempts: number };

export type PollOptions<T> = {
	/** Fetch the current asynchronous-workflow status. */
	getStatus: (signal?: AbortSignal) => Promise<T>;
	/** Return true once no additional status request is needed. */
	isTerminal: (value: T) => boolean;
	/** The delay between requests after the initial request. */
	intervalMs: number;
	/** A hard cap on status requests, including the initial request. */
	maxAttempts: number;
	/** Abort this controller from route/component cleanup on navigation. */
	signal?: AbortSignal;
};

const waitForNextAttempt = (delayMs: number, signal?: AbortSignal): Promise<boolean> =>
	new Promise((resolve) => {
		if (signal?.aborted) {
			resolve(false);
			return;
		}

		const timer = setTimeout(continuePolling, delayMs);
		const abort = () => {
			clearTimeout(timer);
			cleanup();
			resolve(false);
		};
		const cleanup = () => signal?.removeEventListener('abort', abort);
		function continuePolling() {
			cleanup();
			resolve(true);
		}

		signal?.addEventListener('abort', abort, { once: true });
	});

/**
 * Poll an asynchronous operation without orphaning timers. The caller owns an
 * AbortController and aborts it during navigation/component teardown.
 */
export const pollUntilTerminal = async <T>({
	getStatus,
	isTerminal,
	intervalMs,
	maxAttempts,
	signal
}: PollOptions<T>): Promise<PollResult<T>> => {
	if (!Number.isFinite(intervalMs) || intervalMs < 0) {
		throw new RangeError('intervalMs must be a non-negative finite number');
	}
	if (!Number.isInteger(maxAttempts) || maxAttempts < 1) {
		throw new RangeError('maxAttempts must be a positive integer');
	}

	let attempts = 0;
	while (attempts < maxAttempts) {
		if (signal?.aborted) return { kind: 'cancelled', attempts };

		attempts += 1;
		let value: T;
		try {
			value = await getStatus(signal);
		} catch (error) {
			if (signal?.aborted) return { kind: 'cancelled', attempts };
			throw error;
		}

		if (signal?.aborted) return { kind: 'cancelled', attempts };

		if (isTerminal(value)) return { kind: 'completed', value, attempts };
		if (attempts === maxAttempts) return { kind: 'timed-out', attempts };
		if (!(await waitForNextAttempt(intervalMs, signal))) return { kind: 'cancelled', attempts };
	}

	return { kind: 'timed-out', attempts };
};
