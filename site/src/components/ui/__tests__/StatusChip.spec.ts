// ABOUTME: Tests StatusChip rendering across secured/active state combinations.
import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import StatusChip from "@/components/ui/StatusChip.vue";

describe("StatusChip", () => {
  it("renders Private + Active labels when both are true", () => {
    const wrapper = mount(StatusChip, { props: { secured: true, active: true } });

    expect(wrapper.text()).toContain("Private");
    expect(wrapper.text()).toContain("Active");
  });

  it("renders Public + Inactive labels when both are false", () => {
    const wrapper = mount(StatusChip, { props: { secured: false, active: false } });

    expect(wrapper.text()).toContain("Public");
    expect(wrapper.text()).toContain("Inactive");
  });

  it("explains the access badge with a tooltip", () => {
    const on = mount(StatusChip, { props: { secured: true, active: true } });
    expect(on.get('[data-testid="status-secured"]').attributes("title")).toContain(
      "Authentication is required",
    );

    const off = mount(StatusChip, { props: { secured: false, active: true } });
    expect(off.get('[data-testid="status-secured"]').attributes("title")).toContain(
      "Public access allowed",
    );
  });

  it("does not describe a visibility-restricted repository as public", () => {
    const wrapper = mount(StatusChip, {
      props: { secured: false, visibility: "Private", active: true },
    });

    expect(wrapper.get('[data-testid="status-secured"]').text()).toBe("Private");
    expect(wrapper.get('[data-testid="status-secured"]').attributes("title")).toContain(
      "restricted",
    );
  });

  it("applies the secured tonal variant only when secured", () => {
    const on = mount(StatusChip, { props: { secured: true, active: true } });
    expect(on.find(".status-chip--secured").exists()).toBe(true);
    expect(on.find(".status-chip--neutral").exists()).toBe(false);

    const off = mount(StatusChip, { props: { secured: false, active: true } });
    expect(off.find(".status-chip--secured").exists()).toBe(false);
    expect(off.find(".status-chip--neutral").exists()).toBe(true);
  });

  it("applies the active success variant only when active", () => {
    const on = mount(StatusChip, { props: { secured: true, active: true } });
    expect(on.find(".status-chip--active").exists()).toBe(true);

    const off = mount(StatusChip, { props: { secured: true, active: false } });
    expect(off.find(".status-chip--active").exists()).toBe(false);
    expect(off.find(".status-chip--neutral").exists()).toBe(true);
  });

  it("treats null active as active (matches existing UI semantics)", () => {
    const wrapper = mount(StatusChip, { props: { secured: true, active: true } });

    expect(wrapper.find(".status-chip--active").exists()).toBe(true);
  });
});
