import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders } from "../../test/render";
import { ConnectorCreateForm } from "./ConnectorCreateForm";

const maintainer = {
  id: 7,
  display_name: "Platform Team",
  email: "platform@example.test",
  created_at: "2026-07-10T08:00:00Z"
};

const portalUser = {
  id: 11,
  username: "alice",
  roles: ["user"],
  created_at: "2026-07-10T08:00:00Z"
};

describe("ConnectorCreateForm visibility", () => {
  it("requires an explicit visibility choice and exposes friendly team options", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ConnectorCreateForm
        onCreate={vi.fn()}
        onCancel={vi.fn()}
        submitting={false}
        maintainers={[maintainer]}
        users={[portalUser]}
        scopeOptionsLoading={false}
        scopeOptionsError={null}
      />
    );

    expect(screen.getByPlaceholderText("Choose a visibility scope")).toHaveValue("");
    expect(screen.queryByPlaceholderText("Choose a team")).not.toBeInTheDocument();

    await user.click(screen.getByPlaceholderText("Choose a visibility scope"));
    await user.click(screen.getByRole("option", { name: "One maintainer team" }));

    await user.click(screen.getByPlaceholderText("Choose a team"));
    await user.click(screen.getByRole("option", { name: "Platform Team" }));

    expect(document.querySelector<HTMLInputElement>('input[name="scope_type"]')).toHaveValue(
      "maintainer"
    );
    expect(document.querySelector<HTMLInputElement>('input[name="maintainer_id"]')).toHaveValue(
      "7"
    );
    expect(document.querySelector<HTMLInputElement>('input[name="status"]')).toHaveValue("active");
  });

  it("warns before creating globally visible imported data", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ConnectorCreateForm
        onCreate={vi.fn()}
        onCancel={vi.fn()}
        submitting={false}
        maintainers={[]}
        users={[]}
        scopeOptionsLoading={false}
        scopeOptionsError={null}
      />
    );

    await user.click(screen.getByPlaceholderText("Choose a visibility scope"));
    await user.click(screen.getByRole("option", { name: "Everyone in the portal" }));

    expect(
      screen.getByText(/Everyone with portal access will be able to see records/)
    ).toBeInTheDocument();
  });
});
