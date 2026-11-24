<script lang="ts">
	import { displayEvidenceText, displayEvidenceUrlLabel, vettedEvidenceUrl } from '$lib/evidence';

	let { href, label }: { href: string; label?: string } = $props();
	const vettedHref = $derived(vettedEvidenceUrl(href));
	const displayLabel = $derived(
		label === undefined ? displayEvidenceUrlLabel(href) : displayEvidenceText(label, false)
	);
</script>

{#if vettedHref}
	<a href={vettedHref} target="_blank" rel="noopener noreferrer">{displayLabel}</a>
{:else}
	<span>{displayLabel}</span>
{/if}
