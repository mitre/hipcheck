// @ts-nocheck -- executed by Node's built-in TypeScript test runner.
import assert from 'node:assert/strict';
import test from 'node:test';

import { pollUntilTerminal } from '../src/lib/api/polling.ts';

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
