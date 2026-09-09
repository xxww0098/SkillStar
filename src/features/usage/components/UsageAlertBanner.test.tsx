import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import i18n from "../../../i18n";
import type { SubscriptionAlert } from "../types";
import { UsageAlertBanner } from "./UsageAlertBanner";

const alerts: SubscriptionAlert[] = [
  {
    id: "quota-a",
    subscription_id: "account-a",
    severity: "warning",
    kind: "quota-low",
    message: "Account A quota low",
  },
  {
    id: "renew-b",
    subscription_id: "account-b",
    severity: "info",
    kind: "renew-soon",
    message: "Account B renews soon",
  },
  {
    id: "auth-c",
    subscription_id: "account-c",
    severity: "danger",
    kind: "needs-reauth",
    message: "Account C needs reauthentication",
  },
];

describe("UsageAlertBanner", () => {
  it("previews two alerts and reveals every hidden alert without dismissing them", () => {
    const onDismiss = vi.fn();
    render(<UsageAlertBanner alerts={alerts} onDismiss={onDismiss} />);

    expect(screen.getByText(alerts[0].message)).toBeInTheDocument();
    expect(screen.getByText(alerts[1].message)).toBeInTheDocument();
    expect(screen.queryByText(alerts[2].message)).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: i18n.t("usage.expandAlerts", { count: 1 }) }));

    for (const alert of alerts) expect(screen.getByText(alert.message)).toBeInTheDocument();
    expect(onDismiss).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: i18n.t("common.collapse") }));

    expect(screen.getByText(alerts[0].message)).toBeInTheDocument();
    expect(screen.getByText(alerts[1].message)).toBeInTheDocument();
    expect(screen.queryByText(alerts[2].message)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: i18n.t("usage.expandAlerts", { count: 1 }) })).toBeInTheDocument();
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("dismisses the selected hidden alert by identity and removes an unnecessary toggle", () => {
    function DismissibleAlerts() {
      const [remaining, setRemaining] = useState(alerts);
      return (
        <UsageAlertBanner
          alerts={remaining}
          onDismiss={(id) => setRemaining((current) => current.filter((alert) => alert.id !== id))}
        />
      );
    }

    render(<DismissibleAlerts />);
    fireEvent.click(screen.getByRole("button", { name: i18n.t("usage.expandAlerts", { count: 1 }) }));
    fireEvent.click(screen.getAllByRole("button", { name: i18n.t("usage.dismissAlert") })[2]);

    expect(screen.getByText(alerts[0].message)).toBeInTheDocument();
    expect(screen.getByText(alerts[1].message)).toBeInTheDocument();
    expect(screen.queryByText(alerts[2].message)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: i18n.t("common.collapse") })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("only offers expansion when incoming alerts exceed the preview", () => {
    const onDismiss = vi.fn();
    const { container, rerender } = render(<UsageAlertBanner alerts={[]} onDismiss={onDismiss} />);
    expect(container).toBeEmptyDOMElement();

    for (const count of [1, 2]) {
      rerender(<UsageAlertBanner alerts={alerts.slice(0, count)} onDismiss={onDismiss} />);
      for (const alert of alerts.slice(0, count)) expect(screen.getByText(alert.message)).toBeInTheDocument();
      expect(screen.getAllByRole("button")).toHaveLength(count);
      expect(screen.queryByRole("button", { name: i18n.t("common.collapse") })).not.toBeInTheDocument();
    }

    rerender(<UsageAlertBanner alerts={alerts} onDismiss={onDismiss} />);
    expect(screen.getByRole("button", { name: i18n.t("usage.expandAlerts", { count: 1 }) })).toBeInTheDocument();
    expect(screen.queryByText(alerts[2].message)).not.toBeInTheDocument();
  });
});
