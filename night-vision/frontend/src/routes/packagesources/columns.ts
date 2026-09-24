import { createColumnHelper, renderComponent } from "@tanstack/svelte-table";
import type { PackageSourceSummary } from "$lib/api/generated";
import type { DataTableFeatures } from "./data-table-features.js";
import { formatTimestamp } from "$lib/format";
import StatusBadge from "./status-badge.svelte";
import ViewSourceLink from "./view-source-link.svelte";

// Use `accessor` for data columns and `display` for columns without one.
const columnHelper = createColumnHelper<DataTableFeatures, PackageSourceSummary>();

export const columns = columnHelper.columns([
 columnHelper.accessor("displayName", {
  header: "SOURCE",
 }),
 columnHelper.accessor("lifecycle", {
  header: "STATUS",
  cell: (info) => renderComponent(StatusBadge, { status: info.getValue() }),
 }),
 columnHelper.accessor("createdAt", {
  header: "SUBMITTED",
  cell: (info) => formatTimestamp(info.getValue()),
 }),
 columnHelper.accessor("resolutionAt", {
  header: "FINISHED",
  cell: (info) => formatTimestamp(info.getValue()),
 }),
 columnHelper.accessor("reachablePackageCount", {
  header: "REACHABLE PACKAGES",
  cell: (info) => info.getValue() ?? "—",
 }),
 columnHelper.accessor("exposureCount", {
  header: "KEV EXPOSURES",
  // A completed source whose CVE or KEV data is missing has no count.
  cell: (info) => info.getValue() ?? (info.row.original.exposureStatus === "unavailable" ? "Unavailable" : "—"),
 }),
 columnHelper.display({
  id: "action",
  header: "ACTION",
  cell: ({ row }) => renderComponent(ViewSourceLink, { href: `/packagesources/${row.original.id}` }),
 }),
]);
