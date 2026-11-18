<script lang="ts">
	import * as Card from '$lib/components/ui/card/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';
	import { enhance } from '$app/forms';
	import { invalidateAll } from '$app/navigation';
	import { formatTimestamp } from '$lib/format';
	import VerdictBadge from '../verdict-badge.svelte';

	let { data, form } = $props();
	const assessment = $derived(data.assessment);
	let checking = $state<string | null>(null);

	// Reload every two seconds until the assessment finishes.
	$effect(() => {
		if (assessment.status !== 'pending') return;
		const timer = setInterval(() => invalidateAll(), 2000);
		return () => clearInterval(timer);
	});
</script>

<h2 style="color: grey;"><a href="/assessments">Assessments</a> / {assessment.pkg}</h2>
<div class="flex items-center gap-3">
	<h1>Upgrade assessment</h1>
	<VerdictBadge status={assessment.status} verdict={assessment.verdict} />
</div>
<p>
	<span class="font-bold">{assessment.pkg}</span>
	{#each assessment.cves as cve (cve)}
		<Badge variant="destructive" class="ml-1">KEV affected</Badge> {cve}
	{/each}
	{#if assessment.sourceId}
		· from <a href="/sources/{assessment.sourceId}">{assessment.manifestName ?? assessment.fileName ?? 'its package source'}</a>
	{:else if assessment.manifestName ?? assessment.fileName}
		· from {assessment.manifestName ?? assessment.fileName}
	{/if}
	{#if assessment.manifestName && assessment.fileName}
		<span class="text-muted-foreground">({assessment.fileName})</span>
	{/if}
</p>

<Card.Root class="mt-4">
	<Card.Content>
		<div class="flex flex-wrap items-center gap-6 text-sm">
			<div>
				<p class="text-[10px] text-muted-foreground">CANDIDATE</p>
				<p class="font-bold">{assessment.requestedCandidate ?? 'Discovered automatically'}</p>
			</div>
			<Separator orientation="vertical" class="h-10" />
			<div>
				<p class="text-[10px] text-muted-foreground">STARTED</p>
				<p class="font-bold">{formatTimestamp(assessment.createdAt)}</p>
			</div>
			<Separator orientation="vertical" class="h-10" />
			<div>
				<p class="text-[10px] text-muted-foreground">FINISHED</p>
				<p class="font-bold">{formatTimestamp(assessment.completedAt)}</p>
			</div>
		</div>
		{#if assessment.status === 'pending'}
			<p class="mt-4 text-sm">The assessment is running; this page updates when it finishes.</p>
		{:else if assessment.status === 'failed'}
			<p class="mt-4 text-sm text-destructive">The assessment failed: {assessment.error ?? 'no reason was recorded.'}</p>
		{:else}
			<p class="mt-4 font-medium">{assessment.summary}</p>
		{/if}
	</Card.Content>
</Card.Root>

{#if form?.checkError}
	<Alert.Root variant="destructive" class="mt-4">
		<Alert.Description>{form.checkError}</Alert.Description>
	</Alert.Root>
{/if}

{#if assessment.status === 'completed'}
	<h3 class="mt-6">Candidates</h3>
	{#if assessment.candidateCount === 0}
		<p class="text-sm">No newer candidate versions were assessed.</p>
	{:else}
		<p class="text-sm">
			{assessment.candidateCount} candidate{assessment.candidateCount === 1 ? '' : 's'}:
			{assessment.verdictCounts.map(([verdict, count]) => `${count} ${verdict}`).join(' · ')}
			{#if assessment.candidateCount > assessment.candidates.length}
				(showing the nearest {assessment.candidates.length})
			{/if}
		</p>
		<Card.Root class="mt-2">
			<Card.Content>
				<Table.Root>
					<Table.Header>
						<Table.Row>
							<Table.Head>VERSION</Table.Head>
							<Table.Head>UPGRADE</Table.Head>
							<Table.Head>VERDICT</Table.Head>
							{#if !assessment.requestedCandidate}
								<Table.Head class="text-end">SUPPLY-CHAIN CHECK</Table.Head>
							{/if}
						</Table.Row>
					</Table.Header>
					<Table.Body>
						{#each assessment.candidates as candidate (candidate.version)}
							<Table.Row>
								<Table.Cell class="font-bold">{candidate.version}</Table.Cell>
								<Table.Cell>{candidate.upgradeDistance ?? '—'}</Table.Cell>
								<Table.Cell><VerdictBadge verdict={candidate.verdict} /></Table.Cell>
								{#if !assessment.requestedCandidate}
									<Table.Cell class="text-end">
										{#if candidate.verdict !== 'avoid'}
											<form
												method="POST"
												action="?/check"
												use:enhance={() => {
													checking = candidate.version;
													return async ({ update }) => {
														await update();
														checking = null;
													};
												}}
											>
												<input type="hidden" name="candidate" value={candidate.version} />
												<Button type="submit" variant="outline" size="sm" disabled={checking !== null}>
													{checking === candidate.version ? 'Starting…' : 'Check this candidate'}
												</Button>
											</form>
										{:else}
											<span class="text-xs text-muted-foreground">Still affected</span>
										{/if}
									</Table.Cell>
								{/if}
							</Table.Row>
						{/each}
					</Table.Body>
				</Table.Root>
			</Card.Content>
		</Card.Root>
	{/if}

	{#if assessment.findings.length > 0}
		<h3 class="mt-6">Findings</h3>
		<ul class="list-disc pl-6 text-sm">
			{#each assessment.findings as finding (finding.id)}
				<li><span class="font-medium">{finding.title}</span> ({finding.effect}): {finding.summary}</li>
			{/each}
		</ul>
	{:else if assessment.requestedCandidate}
		<p class="mt-4 text-sm">No supply-chain findings were recorded for this candidate.</p>
	{/if}

	{#if assessment.evidence.length > 0}
		<h3 class="mt-6">Evidence</h3>
		<ul class="list-disc pl-6 text-sm">
			{#each assessment.evidence as item (item.id)}
				<li>
					{#if item.url}<a href={item.url} target="_blank" rel="noopener noreferrer">{item.title}</a>{:else}{item.title}{/if}:
					{item.summary}
				</li>
			{/each}
		</ul>
	{/if}

	{#if assessment.caveats.length > 0}
		<h3 class="mt-6">Caveats</h3>
		<ul class="list-disc pl-6 text-sm">
			{#each assessment.caveats as caveat (caveat.code)}
				<li>{caveat.summary}</li>
			{/each}
		</ul>
	{/if}
{/if}
