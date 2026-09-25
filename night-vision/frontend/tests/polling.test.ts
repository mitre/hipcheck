// @ts-nocheck -- executed by Node's built-in TypeScript test runner.
import assert from 'node:assert/strict';
import test from 'node:test';

import { pollUntilTerminal } from '../src/lib/api/polling.ts';
import { isPackageSourceTerminal } from '../src/lib/server/night-vision-api.ts';

type ApiResult<T> = { ok: true; data: T } | { ok: false; error: unknown };

test('stops immediately after a terminal status', async () => {
	let calls = 0;
	const result = await pollUntilTerminal({
		getStatus: async () => {
			calls += 1;
			return 'completed';
		},
		isTerminal: (status) => status === 'completed',
		intervalMs: 0,
		maxAttempts: 3
	});

	assert.deepEqual(result, { kind: 'completed', value: 'completed', attempts: 1 });
	assert.equal(calls, 1);
});

test('stops waiting when route cleanup aborts the poll', async () => {
	const controller = new AbortController();
	let calls = 0;
	const polling = pollUntilTerminal({
		getStatus: async () => {
			calls += 1;
			return 'processing';
		},
		isTerminal: (status) => status !== 'processing',
		intervalMs: 100,
		maxAttempts: 3,
		signal: controller.signal
	});

	controller.abort();
	assert.deepEqual(await polling, { kind: 'cancelled', attempts: 1 });
	assert.equal(calls, 1);
});

test('cancels an in-flight status request when route cleanup aborts the poll', async () => {
	const controller = new AbortController();
	let receivedSignal: AbortSignal | undefined;

	const polling = pollUntilTerminal({
		getStatus: (signal) =>
			new Promise<string>((_resolve, reject) => {
				receivedSignal = signal;
				signal?.addEventListener('abort', () => reject(signal.reason), { once: true });
			}),
		isTerminal: () => false,
		intervalMs: 100,
		maxAttempts: 3,
		signal: controller.signal
	});

	controller.abort(new DOMException('Route cleanup', 'AbortError'));

	assert.equal(receivedSignal, controller.signal);
	assert.deepEqual(await polling, { kind: 'cancelled', attempts: 1 });
});

test('returns a bounded timeout after the maximum number of attempts', async () => {
	let calls = 0;
	const result = await pollUntilTerminal({
		getStatus: async () => {
			calls += 1;
			return 'processing';
		},
		isTerminal: () => false,
		intervalMs: 0,
		maxAttempts: 2
	});

	assert.deepEqual(result, { kind: 'timed-out', attempts: 2 });
	assert.equal(calls, 2);
});

test('composes with ApiResult-based package-source status polling', async () => {
	let calls = 0;
	const statuses: Array<ApiResult<{ status: 'processing' | 'completed'; id: string; createdAt: string }>> = [
		{ ok: true, data: { status: 'processing', id: 'pkg-1', createdAt: '2026-01-01T00:00:00Z' } },
		{ ok: true, data: { status: 'completed', id: 'pkg-1', createdAt: '2026-01-01T00:00:00Z' } }
	];

	const result = await pollUntilTerminal({
		getStatus: async () => {
			const status = statuses[calls];
			calls += 1;
			return status;
		},
		isTerminal: isPackageSourceTerminal,
		intervalMs: 0,
		maxAttempts: 3
	});

	assert.deepEqual(result, {
		kind: 'completed',
		value: { ok: true, data: { status: 'completed', id: 'pkg-1', createdAt: '2026-01-01T00:00:00Z' } },
		attempts: 2
	});
	assert.equal(calls, 2);
});


test('continues polling after retryable ApiResult errors on package-source status checks', async () => {
	let calls = 0;
	const statuses: Array<ApiResult<{ status: 'processing' | 'completed'; id: string; createdAt: string }>> = [
		{
			ok: false,
			error: {
				kind: 'network',
				message: 'Unable to reach Night Vision. Check your connection and try again.',
				retryable: true
			}
		},
		{ ok: true, data: { status: 'completed', id: 'pkg-1', createdAt: '2026-01-01T00:00:00Z' } }
	];

	const result = await pollUntilTerminal({
		getStatus: async () => {
			const status = statuses[calls];
			calls += 1;
			return status;
		},
		isTerminal: isPackageSourceTerminal,
		intervalMs: 0,
		maxAttempts: 3
	});

	assert.deepEqual(result, {
		kind: 'completed',
		value: { ok: true, data: { status: 'completed', id: 'pkg-1', createdAt: '2026-01-01T00:00:00Z' } },
		attempts: 2
	});
	assert.equal(calls, 2);
});

test('treats non-retryable ApiResult errors as terminal polling outcomes', async () => {
	let calls = 0;
	const errorResult: ApiResult<{ status: 'processing'; id: string; createdAt: string }> = {
		ok: false,
		error: {
			kind: 'not-found',
			message: 'The requested resource was not found.',
			retryable: false,
			status: 404
		}
	};

	const result = await pollUntilTerminal({
		getStatus: async () => {
			calls += 1;
			return errorResult;
		},
		isTerminal: isPackageSourceTerminal,
		intervalMs: 0,
		maxAttempts: 3
	});

	assert.deepEqual(result, { kind: 'completed', value: errorResult, attempts: 1 });
	assert.equal(calls, 1);
});
