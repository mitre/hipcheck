<script lang="ts">
 import { Badge } from "$lib/components/ui/badge/index.js";
 import type { PackageSourceSummaryStatus } from "$lib/api/generated";

 let { status }: { status: PackageSourceSummaryStatus } = $props();

 // Colors match the status badges on the example source pages.
 const neutral = "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300";
 const styles: Record<PackageSourceSummaryStatus, { label: string; class: string }> = {
  pending: { label: "Pending", class: neutral },
  processing: { label: "Resolving", class: "bg-blue-50 text-blue-700 dark:bg-blue-950 dark:text-blue-300" },
  completed: { label: "Completed", class: "bg-green-50 text-green-700 dark:bg-green-950 dark:text-green-300" },
  "completed-with-warnings": {
   label: "Warnings",
   class: "bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300",
  },
  cancelled: { label: "Cancelled", class: neutral },
  failed: { label: "Failed", class: "" },
 };
 const style = $derived(styles[status]);
</script>

{#if status === "failed"}
 <Badge variant="destructive" class="min-w-24">{style.label}</Badge>
{:else}
 <Badge class="min-w-24 {style.class}">{style.label}</Badge>
{/if}
