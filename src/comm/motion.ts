export const MOTION_EASE_STANDARD = [0.22, 1, 0.36, 1] as const;
export const MOTION_EASE_EMPHASIS = [0.16, 1, 0.3, 1] as const;

export const MOTION_DURATION = {
  instant: 0.01,
  fast: 0.15,
  base: 0.2,
  medium: 0.3,
  modal: 0.35,
  progress: 0.4,
  ring: 0.8,
} as const;

export const MOTION_TRANSITION = {
  fadeFast: { duration: MOTION_DURATION.fast, ease: MOTION_EASE_STANDARD },
  fadeBase: { duration: MOTION_DURATION.base, ease: MOTION_EASE_STANDARD },
  fadeMedium: { duration: MOTION_DURATION.medium, ease: MOTION_EASE_STANDARD },
  enter: { duration: MOTION_DURATION.base, ease: MOTION_EASE_STANDARD },
  collapse: { duration: MOTION_DURATION.base, ease: MOTION_EASE_STANDARD },
  modalBackdrop: {
    duration: MOTION_DURATION.medium,
    ease: MOTION_EASE_STANDARD,
  },
  modal: { duration: MOTION_DURATION.modal, ease: MOTION_EASE_EMPHASIS },
  progress: { duration: MOTION_DURATION.progress, ease: "easeOut" as const },
  ring: { duration: MOTION_DURATION.ring, ease: MOTION_EASE_STANDARD },
} as const;

export const motionDelay = (index: number, step = 0.03, max = 0.3) =>
  Math.min(index * step, max);

export const motionDuration = (
  prefersReducedMotion: boolean | null | undefined,
  duration: number,
) => (prefersReducedMotion ? MOTION_DURATION.instant : duration);
