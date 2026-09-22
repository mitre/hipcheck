
import { createColumnHelper, renderComponent, renderSnippet } from "@tanstack/svelte-table";
import { createRawSnippet } from "svelte";
import type { DataTableFeatures } from "./data-table-features.js";
import DataTableActions from "./data-table-actions.svelte";
import DataTableEmailButton from "./data-table-email-button.svelte";


 // This type is used to define the shape of our data.
// You can use a Zod schema here if you want.
export type Payment = {
 id: string;
 amount: number;
 status: "pending" | "processing" | "success" | "failed";
 email: string;
};

const columnHelper = createColumnHelper<DataTableFeatures, Payment>();




export const columns = columnHelper.columns([
 columnHelper.accessor("status", {
  header: "Status",
 }),
 columnHelper.accessor("email", {
  header: "Email",
 }),
 columnHelper.accessor("amount", {
  header: () => {
   const amountHeaderSnippet = createRawSnippet(() => ({
    render: () => `<div class="text-end">Amount</div>`,
   }));
   return renderSnippet(amountHeaderSnippet);
  },
  cell: ({ row }) => {
   const formatter = new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
   });
 
   const amountCellSnippet = createRawSnippet<[{ amount: number }]>(
    (getAmount) => {
     const { amount } = getAmount();
     const formatted = formatter.format(amount);
     return {
      render: () =>
       `<div class="text-end font-medium">${formatted}</div>`,
     };
    }
   );
 
   return renderSnippet(amountCellSnippet, {
    amount: row.original.amount,
   });
  },
 }),
 
]);