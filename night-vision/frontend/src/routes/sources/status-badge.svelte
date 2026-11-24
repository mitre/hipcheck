<script lang="ts">
 import { Badge } from "$lib/components/ui/badge/index.js";
 import type { PackageSourceLifecycle } from "$lib/api/generated";

 let { status }: { status: PackageSourceLifecycle } = $props();

 const neutral = "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300";
 const styles: Record<PackageSourceLifecycle, { label: string; class: string }> = {
  pending: { label: "Pending", class: neutral },
  processing: { label: "Processing", class: "bg-blue-50 text-blue-700 dark:bg-blue-950 dark:text-blue-300" },
  completed: { label: "Completed", class: "bg-green-50 text-green-700 dark:bg-green-950 dark:text-green-300" },
  "completed-with-warnings": {
   label: "Completed with warnings",
   class: "bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300",
  },
  cancelled: { label: "Cancelled", class: neutral },
  failed: { label: "Failed", class: "" },
 };
 const style = $derived(styles[status]);
 const icons: Record<PackageSourceLifecycle, string> = {
  pending: "◷",
  processing: "↻",
  completed: "✓",
  "completed-with-warnings": "⚠",
  cancelled: "⊘",
  failed: "!",
 };
</script>

{#if status === "failed"}
 <Badge variant="destructive"><span aria-hidden="true">{icons[status]}</span> {style.label}</Badge>
{:else}
 <Badge class={style.class}><span aria-hidden="true">{icons[status]}</span> {style.label}</Badge>
{/if}
