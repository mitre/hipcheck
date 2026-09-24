<script lang="ts" generics="TData extends RowData">
 import {
  type ColumnDef,
  type RowData,
  createTable,
  FlexRender,
 } from "@tanstack/svelte-table";
 import * as Table from "$lib/components/ui/table/index.js";
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
  get columns() {
   return columns;
  },
 });
</script>
 
<div class="rounded-md border">
 <Table.Root>
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