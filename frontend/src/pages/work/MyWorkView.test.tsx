import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createMockApiClient } from "../../test/mockApiClient";
import { renderWithProviders } from "../../test/render";
import type { MyWorkResponse, WorkCard } from "../../types/api";
import { MyWorkView } from "./MyWorkView";

describe("MyWorkView", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "/#my-work");
  });

  it("loads the URL filters and renders actionable assignment context", async () => {
    const path =
      "/me/work-cards?status=blocked&due=overdue&project=Portal&work_item_type=Bug" +
      "&source=azure-devops&sort=attention&page=1&page_size=25";
    const { client, calls } = createMockApiClient({ [path]: response([blockedCard()], 1) });
    const onNavigate = vi.fn();
    const onOpenWorkCard = vi.fn();
    const search =
      "source=azure-devops&work_item_type=Bug&project=Portal&due=overdue&status=blocked";
    window.history.replaceState(null, "", `/#my-work?${search}`);

    renderWithProviders(
      <MyWorkView
        client={client}
        searchParams={new URLSearchParams(search)}
        onNavigate={onNavigate}
        onOpenWorkCard={onOpenWorkCard}
      />
    );

    expect(await screen.findByText("Fix production deployment")).toBeVisible();
    expect(calls[0]).toEqual({ method: "GET", path });
    expect(window.location.hash).toBe(
      "#my-work?status=blocked&due=overdue&project=Portal&work_item_type=Bug&source=azure-devops"
    );
    expect(screen.getByText("Portal · Bug · azure-devops")).toBeVisible();
    expect(screen.getByText("overdue")).toBeVisible();
    expect(screen.getByText(/^Source updated 2026/)).toBeVisible();
    expect(screen.getByRole("link", { name: "External card" })).toHaveAttribute(
      "href",
      "https://dev.azure.test/workitems/42"
    );

    await userEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(onOpenWorkCard).toHaveBeenCalledWith(
      42,
      "#work-cards/42?from=my-work&status=blocked&due=overdue&project=Portal" +
        "&work_item_type=Bug&source=azure-devops"
    );

    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(calls).toHaveLength(2));
  });

  it("distinguishes filtered and unfiltered empty results", async () => {
    const filteredPath =
      "/me/work-cards?status=blocked&sort=attention&page=1&page_size=25";
    const { client } = createMockApiClient({ [filteredPath]: response([], 0) });

    renderWithProviders(
      <MyWorkView
        client={client}
        searchParams={new URLSearchParams("status=blocked")}
        onNavigate={vi.fn()}
        onOpenWorkCard={vi.fn()}
      />
    );

    expect(await screen.findByText("No assigned work matches these filters.")).toBeVisible();
  });

  it("keeps pagination in the navigable hash", async () => {
    const path = "/me/work-cards?sort=attention&page=1&page_size=25";
    const { client } = createMockApiClient({ [path]: response([blockedCard()], 30) });
    const onNavigate = vi.fn();

    renderWithProviders(
      <MyWorkView
        client={client}
        searchParams={new URLSearchParams()}
        onNavigate={onNavigate}
        onOpenWorkCard={vi.fn()}
      />
    );

    expect(await screen.findByText("1-25 of 30")).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "2" }));
    expect(onNavigate).toHaveBeenCalledWith("#my-work?page=2");
  });

  it("shows request errors and allows an explicit retry", async () => {
    const path = "/me/work-cards?sort=attention&page=1&page_size=25";
    const { client, calls } = createMockApiClient({
      [path]: () => Promise.reject(new Error("Work service unavailable"))
    });

    renderWithProviders(
      <MyWorkView
        client={client}
        searchParams={new URLSearchParams()}
        onNavigate={vi.fn()}
        onOpenWorkCard={vi.fn()}
      />
    );

    expect(await screen.findByText("Work service unavailable")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(calls).toHaveLength(2));
  });
});

function blockedCard(): WorkCard {
  return {
    id: 42,
    source: "azure-devops",
    external_id: "42",
    title: "Fix production deployment",
    status: "blocked",
    priority: "urgent",
    assignee: "Taylor Lin",
    assignee_source_id: "aad.taylor",
    assignee_user_id: 7,
    project: "Portal",
    work_item_type: "Bug",
    due_at: "2020-01-01T00:00:00Z",
    source_updated_at: "2026-07-11T08:30:00Z",
    url: "https://dev.azure.test/workitems/42",
    created_at: "2026-07-01T00:00:00Z",
    updated_at: "2026-07-11T08:31:00Z"
  };
}

function response(items: WorkCard[], total: number): MyWorkResponse {
  return {
    items,
    total,
    page: 1,
    page_size: 25,
    facets: {
      statuses: ["blocked", "in_progress"],
      projects: ["Portal"],
      work_item_types: ["Bug"],
      sources: ["azure-devops"]
    }
  };
}
