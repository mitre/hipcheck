import { createColumnHelper } from "@tanstack/svelte-table";
import type { DataTableFeatures } from "./data-table-features.js";
 
// This type is used to define the shape of our data.
// You can use a Zod schema here if you want.
export type Payment = {
 id: string;
 amount: number;
 status: "pending" | "processing" | "success" | "failed";
 email: string;
};
 
// Use `accessor` for data columns and `display` for columns without one.
const columnHelper = createColumnHelper<DataTableFeatures, Payment>();
 
export const columns = columnHelper.columns([
 columnHelper.accessor("status", {
  header: "Status",
 }),
 columnHelper.accessor("email", {
  header: "Email",
 }),
 columnHelper.accessor("amount", {
  header: "Amount",
 }),
]);