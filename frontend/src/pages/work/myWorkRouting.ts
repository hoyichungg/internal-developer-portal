import type { ApiId } from "../../types/api";

export const MY_WORK_DUE_VALUES = ["overdue", "today", "next_7_days", "none"] as const;
export const MY_WORK_SORT_VALUES = [
  "attention",
  "due_asc",
  "source_updated_desc"
] as const;
export const MY_WORK_PAGE_SIZES = [10, 25, 50, 100] as const;

export type MyWorkDue = "" | (typeof MY_WORK_DUE_VALUES)[number];
export type MyWorkSort = (typeof MY_WORK_SORT_VALUES)[number];

export type MyWorkQuery = {
  status: string;
  due: MyWorkDue;
  project: string;
  workItemType: string;
  source: string;
  sort: MyWorkSort;
  page: number;
  pageSize: number;
};

export const DEFAULT_MY_WORK_QUERY: MyWorkQuery = {
  status: "",
  due: "",
  project: "",
  workItemType: "",
  source: "",
  sort: "attention",
  page: 1,
  pageSize: 25
};

export function parseMyWorkQuery(input: URLSearchParams | string): MyWorkQuery {
  const params =
    typeof input === "string"
      ? new URLSearchParams(input.replace(/^\?/, ""))
      : new URLSearchParams(input);

  return {
    status: boundedValue(params.get("status")),
    due: enumValue(params.get("due"), MY_WORK_DUE_VALUES, ""),
    project: boundedValue(params.get("project")),
    workItemType: boundedValue(params.get("work_item_type")),
    source: boundedValue(params.get("source")),
    sort: enumValue(params.get("sort"), MY_WORK_SORT_VALUES, DEFAULT_MY_WORK_QUERY.sort),
    page: positiveInteger(params.get("page"), DEFAULT_MY_WORK_QUERY.page),
    pageSize: pageSize(params.get("page_size"))
  };
}

export function myWorkHash(query: MyWorkQuery): string {
  const params = myWorkParams(query, false);
  const search = params.toString();
  return search ? `#my-work?${search}` : "#my-work";
}

export function myWorkApiPath(query: MyWorkQuery): string {
  const params = myWorkParams(query, true);
  return `/me/work-cards?${params.toString()}`;
}

export function myWorkDetailHash(workCardId: ApiId, query: MyWorkQuery): string {
  const params = new URLSearchParams({ from: "my-work" });
  const returnSearch = myWorkHash(query).split("?", 2)[1];

  if (returnSearch) {
    new URLSearchParams(returnSearch).forEach((value, key) => params.set(key, value));
  }

  return `#work-cards/${workCardId}?${params.toString()}`;
}

export function myWorkReturnHash(params: URLSearchParams): string | null {
  if (params.get("from") !== "my-work") {
    return null;
  }

  return myWorkHash(parseMyWorkQuery(params));
}

export function safeMyWorkQuery(query: string): string {
  return myWorkHash(parseMyWorkQuery(query)).split("?", 2)[1] || "";
}

export function safeMyWorkDetailQuery(query: string): string {
  const incoming = new URLSearchParams(query);
  if (incoming.get("from") !== "my-work") {
    return "";
  }

  const outgoing = new URLSearchParams({ from: "my-work" });
  const safeQuery = safeMyWorkQuery(query);
  if (safeQuery) {
    new URLSearchParams(safeQuery).forEach((value, key) => outgoing.set(key, value));
  }

  return outgoing.toString();
}

function myWorkParams(query: MyWorkQuery, includeDefaults: boolean): URLSearchParams {
  const normalized = parseMyWorkQuery(
    new URLSearchParams({
      status: query.status,
      due: query.due,
      project: query.project,
      work_item_type: query.workItemType,
      source: query.source,
      sort: query.sort,
      page: String(query.page),
      page_size: String(query.pageSize)
    })
  );
  const params = new URLSearchParams();

  setWhenPresent(params, "status", normalized.status);
  setWhenPresent(params, "due", normalized.due);
  setWhenPresent(params, "project", normalized.project);
  setWhenPresent(params, "work_item_type", normalized.workItemType);
  setWhenPresent(params, "source", normalized.source);

  if (includeDefaults || normalized.sort !== DEFAULT_MY_WORK_QUERY.sort) {
    params.set("sort", normalized.sort);
  }
  if (includeDefaults || normalized.page !== DEFAULT_MY_WORK_QUERY.page) {
    params.set("page", String(normalized.page));
  }
  if (includeDefaults || normalized.pageSize !== DEFAULT_MY_WORK_QUERY.pageSize) {
    params.set("page_size", String(normalized.pageSize));
  }

  return params;
}

function setWhenPresent(params: URLSearchParams, key: string, value: string) {
  if (value) {
    params.set(key, value);
  }
}

function boundedValue(value: string | null): string {
  const normalized = value?.trim() || "";
  if (!normalized || normalized.length > 128 || /[\u0000-\u001f\u007f]/.test(normalized)) {
    return "";
  }

  return normalized;
}

function enumValue<T extends string, F extends T | "">(
  value: string | null,
  allowed: readonly T[],
  fallback: F
): T | F {
  return value && allowed.includes(value as T) ? (value as T) : fallback;
}

function positiveInteger(value: string | null, fallback: number): number {
  if (!value || !/^\d+$/.test(value)) {
    return fallback;
  }

  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 && parsed <= 1_000_000 ? parsed : fallback;
}

function pageSize(value: string | null): number {
  const parsed = positiveInteger(value, DEFAULT_MY_WORK_QUERY.pageSize);
  return MY_WORK_PAGE_SIZES.includes(parsed as (typeof MY_WORK_PAGE_SIZES)[number])
    ? parsed
    : DEFAULT_MY_WORK_QUERY.pageSize;
}
