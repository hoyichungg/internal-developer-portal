import { describe, expect, it } from "vitest";

import {
  DEFAULT_MY_WORK_QUERY,
  myWorkApiPath,
  myWorkDetailHash,
  myWorkHash,
  myWorkReturnHash,
  parseMyWorkQuery,
  safeMyWorkDetailQuery,
  safeMyWorkQuery
} from "./myWorkRouting";

describe("My Work routing", () => {
  it("parses every supported filter and canonicalizes the hash in stable order", () => {
    const query = parseMyWorkQuery(
      "source=azure-devops&page_size=50&work_item_type=Bug&status=Blocked&due=next_7_days" +
        "&project=Portal&page=3&sort=source_updated_desc&ignored=secret"
    );

    expect(query).toEqual({
      status: "Blocked",
      due: "next_7_days",
      project: "Portal",
      workItemType: "Bug",
      source: "azure-devops",
      sort: "source_updated_desc",
      page: 3,
      pageSize: 50
    });
    expect(myWorkHash(query)).toBe(
      "#my-work?status=Blocked&due=next_7_days&project=Portal&work_item_type=Bug" +
        "&source=azure-devops&sort=source_updated_desc&page=3&page_size=50"
    );
  });

  it("fails closed to bounded defaults for unsupported or malformed values", () => {
    const query = parseMyWorkQuery(
      `status=${"x".repeat(129)}&due=tomorrow&sort=priority&page=-2&page_size=99&source=ok%0Abad`
    );

    expect(query).toEqual(DEFAULT_MY_WORK_QUERY);
    expect(myWorkHash(query)).toBe("#my-work");
    expect(myWorkApiPath(query)).toBe(
      "/me/work-cards?sort=attention&page=1&page_size=25"
    );
  });

  it("keeps the canonical filters through detail navigation and back", () => {
    const query = parseMyWorkQuery(
      "status=blocked&due=overdue&project=Platform&work_item_type=Bug&page=2"
    );
    const detailHash = myWorkDetailHash(42, query);

    expect(detailHash).toBe(
      "#work-cards/42?from=my-work&status=blocked&due=overdue&project=Platform" +
        "&work_item_type=Bug&page=2"
    );
    expect(myWorkReturnHash(new URLSearchParams(detailHash.split("?", 2)[1]))).toBe(
      "#my-work?status=blocked&due=overdue&project=Platform&work_item_type=Bug&page=2"
    );
  });

  it("exposes only allowlisted My Work values to auth return-to routing", () => {
    expect(
      safeMyWorkQuery(
        "source=ado&due=today&page=2&page_size=100&sort=due_asc&token=secret"
      )
    ).toBe("due=today&source=ado&sort=due_asc&page=2&page_size=100");
    expect(
      safeMyWorkDetailQuery(
        "from=my-work&status=blocked&project=Portal&redirect=https%3A%2F%2Fevil.test"
      )
    ).toBe("from=my-work&status=blocked&project=Portal");
    expect(safeMyWorkDetailQuery("from=dashboard&status=blocked")).toBe("");
  });
});
