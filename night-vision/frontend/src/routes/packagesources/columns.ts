import { createColumnHelper, renderComponent } from "@tanstack/svelte-table";
import type { PackageSourceSummary } from "$lib/api/generated";
import type { DataTableFeatures } from "./data-table-features.js";
import { formatFailureKind, formatTimestamp } from "./format";
import StatusBadge from "./status-badge.svelte";
import ViewSourceLink from "./view-source-link.svelte";

/** A list row plus its KEV exposure count; null when not known (not completed, or lookup failed). */
export type PackageSourceRow = PackageSourceSummary & { kevExposures: number | null };

// Use `accessor` for data columns and `display` for columns without one.
const columnHelper = createColumnHelper<DataTableFeatures, PackageSourceRow>();

export const columns = columnHelper.columns([
 columnHelper.accessor("fileName", {
  header: "FILE",
 }),
 columnHelper.accessor("status", {
  header: "STATUS",
  cell: (info) => renderComponent(StatusBadge, { status: info.getValue() }),
 }),
 columnHelper.accessor("createdAt", {
  header: "SUBMITTED",
  cell: (info) => formatTimestamp(info.getValue()),
 }),
 columnHelper.accessor("finishedAt", {
  header: "FINISHED",
  cell: (info) => formatTimestamp(info.getValue()),
 }),
 columnHelper.accessor("kevExposures", {
  header: "KEV EXPOSURES",
  cell: (info) => info.getValue() ?? "—",
 }),
 columnHelper.accessor("attempt", {
  header: "ATTEMPT",
 }),
 columnHelper.accessor("failureKind", {
  header: "FAILURE",
  cell: (info) => formatFailureKind(info.getValue()),
 }),
 columnHelper.display({
  id: "action",
  header: "ACTION",
  cell: ({ row }) => renderComponent(ViewSourceLink, { href: `/packagesources/${row.original.id}` }),
 }),
]);
