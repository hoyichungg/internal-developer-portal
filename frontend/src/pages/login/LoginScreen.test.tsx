import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders } from "../../test/render";
import type { PublicAuthConfig } from "../../types/api";
import { LoginScreen } from "./LoginScreen";

describe("LoginScreen sign-in methods", () => {
  it("submits credentials when password is the only enabled method", async () => {
    const onLogin = vi.fn().mockResolvedValue(undefined);
    renderLogin({ password_login_enabled: true, entra_login_enabled: false }, { onLogin });

    const user = userEvent.setup();
    await user.type(screen.getByLabelText(/Username/), "portal-user");
    await user.type(screen.getByLabelText(/Password/), "correct-password");
    await user.click(screen.getByRole("button", { name: "Sign in with password" }));

    expect(onLogin).toHaveBeenCalledWith({
      username: "portal-user",
      password: "correct-password"
    });
    expect(
      screen.queryByRole("link", { name: "Continue with Microsoft" })
    ).not.toBeInTheDocument();
  });

  it("shows only the Entra link when local password login is disabled", () => {
    renderLogin({ password_login_enabled: false, entra_login_enabled: true });

    expect(screen.getByRole("link", { name: "Continue with Microsoft" })).toHaveAttribute(
      "href",
      "/auth/entra/start?return_to=%2F%23catalog"
    );
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/Password/)).not.toBeInTheDocument();
  });

  it("shows Entra and password with a separator when both methods are enabled", () => {
    renderLogin({ password_login_enabled: true, entra_login_enabled: true });

    expect(screen.getByRole("link", { name: "Continue with Microsoft" })).toBeVisible();
    expect(screen.getByText("or")).toBeVisible();
    expect(screen.getByLabelText(/Username/)).toBeVisible();
    expect(screen.getByLabelText(/Password/)).toBeVisible();
  });

  it("fails closed while config is loading, when it fails, or when no method is enabled", async () => {
    const loading = renderLogin(null);
    expect(screen.getByRole("status")).toHaveTextContent("Loading sign-in options");
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();
    loading.unmount();

    const retry = vi.fn();
    const failed = renderLogin(null, {
      authConfigError: new Error("Portal API unavailable"),
      onRetryAuthConfig: retry
    });
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledOnce();
    expect(screen.queryByLabelText(/Username/)).not.toBeInTheDocument();
    failed.unmount();

    renderLogin({ password_login_enabled: false, entra_login_enabled: false });
    expect(
      screen.getByText("No sign-in method is enabled. Contact the portal administrator.")
    ).toBeVisible();
    expect(
      screen.queryByRole("link", { name: "Continue with Microsoft" })
    ).not.toBeInTheDocument();
  });

  it("shows only the safe callback message supplied by App", () => {
    renderLogin(
      { password_login_enabled: false, entra_login_enabled: true },
      { callbackError: "Microsoft sign-in was cancelled or denied." }
    );

    expect(screen.getByRole("alert")).toHaveTextContent("Microsoft sign-in failed");
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Microsoft sign-in was cancelled or denied."
    );
  });
});

function renderLogin(
  authConfig: PublicAuthConfig | null,
  overrides: Partial<ComponentProps<typeof LoginScreen>> = {}
) {
  return renderWithProviders(loginScreen(authConfig, overrides));
}

function loginScreen(
  authConfig: PublicAuthConfig | null,
  overrides: Partial<ComponentProps<typeof LoginScreen>> = {}
) {
  return (
    <LoginScreen
      authConfig={authConfig}
      authConfigError={null}
      callbackError={null}
      entraLoginUrl="/auth/entra/start?return_to=%2F%23catalog"
      onLogin={vi.fn().mockResolvedValue(undefined)}
      onRetryAuthConfig={vi.fn()}
      {...overrides}
    />
  );
}
