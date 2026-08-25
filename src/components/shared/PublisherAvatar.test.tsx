import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { PublisherAvatar } from "./PublisherAvatar";

describe("PublisherAvatar", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("hides fallback complexity behind the identity-and-size interface", () => {
    render(<PublisherAvatar name="example-publisher" size="lg" />);

    const localImage = screen.getByRole("img", { name: "example-publisher" });
    expect(localImage).toHaveAttribute("src", "/publishers/example-publisher.png");

    fireEvent.error(localImage);
    const remoteImage = screen.getByRole("img", { name: "example-publisher" });
    expect(remoteImage).toHaveAttribute("src", "https://github.com/example-publisher.png?size=120");

    fireEvent.error(remoteImage);
    expect(screen.queryByRole("img", { name: "example-publisher" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("example-publisher publisher")).toBeInTheDocument();
  });

  it("reuses a cached fallback so remounts skip the local-then-remote walk", () => {
    const first = render(<PublisherAvatar name="cached-pub" />);
    fireEvent.error(screen.getByRole("img", { name: "cached-pub" }));
    fireEvent.error(screen.getByRole("img", { name: "cached-pub" }));
    first.unmount();

    render(<PublisherAvatar name="cached-pub" />);
    expect(screen.queryByRole("img", { name: "cached-pub" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("cached-pub publisher")).toBeInTheDocument();
  });
});
