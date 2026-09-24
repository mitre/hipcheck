<script lang="ts" generics="TData extends RowData">
 import {
  type ColumnDef,
  type RowData,
  createTable,
  FlexRender,
 } from "@tanstack/svelte-table";
 import * as Table from "$lib/components/ui/table/index.js";
 import { Button } from "$lib/components/ui/button/index.js";
 import { features, type DataTableFeatures } from "./data-table-features.js";
 
 type DataTableProps<TData extends RowData> = {
  columns: ColumnDef<DataTableFeatures, TData>[];
  data: TData[];
 };
 
 let { data, columns }: DataTableProps<TData> = $props();
 
 const table = createTable({
  features,
  get data() {
   return data;
  },
  columns,
 });

 const pagination = $derived(table.atoms.pagination.get());
 const rowCount = $derived(table.getRowCount());
 const firstShown = $derived(rowCount === 0 ? 0 : pagination.pageIndex * pagination.pageSize + 1);
 const lastShown = $derived(Math.min((pagination.pageIndex + 1) * pagination.pageSize, rowCount));
</script>
 
<div>
 <Table.Root class="min-w-[1400px] table-fixed">
  <Table.Header>
   {#each table.getHeaderGroups() as headerGroup (headerGroup.id)}
    <Table.Row>
     {#each headerGroup.headers as header (header.id)}
      <Table.Head colspan={header.colSpan}>
       {#if !header.isPlaceholder}
        <FlexRender {header} />
       {/if}
      </Table.Head>
     {/each}
    </Table.Row>
   {/each}
  </Table.Header>
  <Table.Body>
   {#each table.getRowModel().rows as row (row.id)}
    <Table.Row data-state={row.getIsSelected() && "selected"}>
     {#each row.getVisibleCells() as cell (cell.id)}
      <Table.Cell>
       <FlexRender {cell} />
      </Table.Cell>
     {/each}
    </Table.Row>
   {:else}
    <Table.Row>
     <Table.Cell colspan={columns.length} class="h-24 text-center">
      No results.
     </Table.Cell>
    </Table.Row>
   {/each}
  </Table.Body>
 </Table.Root>
</div>

<div class="flex items-center justify-between py-4">
 <p class="text-sm text-muted-foreground">
  Showing {firstShown}–{lastShown} of {rowCount}
 </p>
 <div class="flex items-center gap-2">
  <Button
   variant="outline"
   size="sm"
   onclick={() => table.previousPage()}
   disabled={!table.getCanPreviousPage()}
  >
   Previous
  </Button>
  <span class="text-sm">
   Page {pagination.pageIndex + 1} of {Math.max(table.getPageCount(), 1)}
  </span>
  <Button
   variant="outline"
   size="sm"
   onclick={() => table.nextPage()}
   disabled={!table.getCanNextPage()}
  >
   Next
  </Button>
 </div>
</div>
