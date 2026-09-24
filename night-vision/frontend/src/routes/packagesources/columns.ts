import { createColumnHelper } from "@tanstack/svelte-table";
import type { PackageSourceSummary } from "$lib/api/generated";
import type { DataTableFeatures } from "./data-table-features.js";

const formatTimestamp = (value: string | null | undefined): string =>
 value ? new Date(value).toLocaleString() : "—";

// Use `accessor` for data columns and `display` for columns without one.
const columnHelper = createColumnHelper<DataTableFeatures, PackageSourceSummary>();

export const columns = columnHelper.columns([
 columnHelper.accessor("fileName", {
  header: "File",
 }),
 columnHelper.accessor("status", {
  header: "Status",
 }),
 columnHelper.accessor("createdAt", {
  header: "Submitted",
  cell: (info) => formatTimestamp(info.getValue()),
 }),
 columnHelper.accessor("finishedAt", {
  header: "Finished",
  cell: (info) => formatTimestamp(info.getValue()),
 }),
 columnHelper.accessor("attempt", {
  header: "Attempt",
 }),
 columnHelper.accessor("failureKind", {
  header: "Failure",
  cell: (info) => info.getValue() ?? "—",
 }),
]);
