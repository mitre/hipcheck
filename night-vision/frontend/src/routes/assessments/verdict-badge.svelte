<script lang="ts">
	import { Badge } from '$lib/components/ui/badge/index.js';
	import type { UpgradeAssessmentVerdict, UpgradeAssessmentWorkflowStatus } from '$lib/api/generated';

	let {
		status = 'completed',
		verdict
	}: { status?: UpgradeAssessmentWorkflowStatus; verdict: UpgradeAssessmentVerdict | null | undefined } = $props();

	// Colors match the verdict badges on the example assessment pages.
	const styles: Record<Exclude<UpgradeAssessmentVerdict, 'avoid'>, { label: string; class: string }> = {
		recommended: { label: 'Recommended', class: 'bg-green-50 text-green-800 dark:bg-green-950 dark:text-green-300' },
		caution: { label: 'Caution', class: 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300' },
		unknown: { label: 'Unknown', class: 'bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300' }
	};
</script>

{#if status === 'pending'}
	<Badge class="min-w-24 bg-blue-50 text-blue-700 dark:bg-blue-950 dark:text-blue-300">Running</Badge>
{:else if status === 'failed'}
	<Badge variant="destructive" class="min-w-24">Failed</Badge>
{:else if verdict === 'avoid'}
	<Badge variant="destructive" class="min-w-24">Avoid</Badge>
{:else if verdict}
	<Badge class="min-w-24 {styles[verdict].class}">{styles[verdict].label}</Badge>
{:else}
	<span>—</span>
{/if}
