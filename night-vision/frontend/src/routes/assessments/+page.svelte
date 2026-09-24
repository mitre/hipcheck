<script lang="ts">
	import * as Table from '$lib/components/ui/table/index.js';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { invalidateAll } from '$app/navigation';
	import { formatTimestamp } from '$lib/format';
	import VerdictBadge from './verdict-badge.svelte';

	let { data } = $props();
	const running = $derived(data.assessments.some((assessment) => assessment.status === 'pending'));

	// Reload every two seconds while any assessment is still running.
	$effect(() => {
		if (!running) return;
		const timer = setInterval(() => invalidateAll(), 2000);
		return () => clearInterval(timer);
	});
</script>

<h1>Assessments</h1>
<p>Upgrade assessments for KEV-linked exposures in your package sources.</p>

{#if data.assessments.length === 0}
	<p class="mt-4 text-sm">
		No assessments yet. Open a package source with a KEV exposure and choose <strong>Run assessment</strong>.
	</p>
{:else}
	<Table.Root class="mt-4">
		<Table.Header>
			<Table.Row>
				<Table.Head>EXPOSURE</Table.Head>
				<Table.Head>SOURCE</Table.Head>
				<Table.Head>CANDIDATE</Table.Head>
				<Table.Head>VERDICT</Table.Head>
				<Table.Head>SUMMARY</Table.Head>
				<Table.Head>UPDATED</Table.Head>
				<Table.Head class="text-end">ACTION</Table.Head>
			</Table.Row>
		</Table.Header>
		<Table.Body>
			{#each data.assessments as assessment (assessment.id)}
				<Table.Row>
					<Table.Cell>
						<p class="font-bold">{assessment.pkg}</p>
						{#if assessment.cve}
							<Badge variant="destructive">KEV affected</Badge>
							{assessment.cve}
						{/if}
					</Table.Cell>
					<Table.Cell>
						{#if assessment.sourceId}
							<a class="font-medium underline" href="/packagesources/{assessment.sourceId}">{assessment.sourceName}</a>
						{:else}
							{assessment.sourceName}
						{/if}
						{#if assessment.fileName !== assessment.sourceName}
							<p class="text-xs text-muted-foreground">{assessment.fileName}</p>
						{/if}
					</Table.Cell>
					<Table.Cell>{assessment.candidate ?? 'Discovered automatically'}</Table.Cell>
					<Table.Cell><VerdictBadge status={assessment.status} verdict={assessment.verdict} /></Table.Cell>
					<Table.Cell class="whitespace-normal">{assessment.summary ?? '—'}</Table.Cell>
					<Table.Cell>{formatTimestamp(assessment.updatedAt)}</Table.Cell>
					<Table.Cell class="text-end">
						<Button href="/assessments/{assessment.id}" variant="outline" size="sm">Open</Button>
					</Table.Cell>
				</Table.Row>
			{/each}
		</Table.Body>
	</Table.Root>
{/if}
<p class="mt-2 text-xs text-muted-foreground">
	Night Vision can't list assessments yet, so this page shows the ones started from this browser.
</p>
