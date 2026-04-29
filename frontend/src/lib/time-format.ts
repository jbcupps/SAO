/**
 * Render a timestamp as a relative phrase ("12 seconds ago", "in 3 hours").
 * Returns "never seen" for null/undefined/invalid input so callers can
 * surface "agent has no egress yet" without a separate branch.
 *
 * Buckets:
 *   < 60s     → seconds
 *   < 60m     → minutes
 *   < 24h     → hours
 *   otherwise → days
 *
 * The `now` parameter exists so unit tests can control the reference clock.
 */
export function formatRelativeTime(
  iso: string | null | undefined,
  now: Date = new Date(),
): string {
  if (!iso) return 'never seen';
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return 'never seen';

  const diffSec = Math.round((then.getTime() - now.getTime()) / 1000);
  const fmt = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  const abs = Math.abs(diffSec);

  if (abs < 60) return fmt.format(diffSec, 'second');
  if (abs < 3600) return fmt.format(Math.round(diffSec / 60), 'minute');
  if (abs < 86_400) return fmt.format(Math.round(diffSec / 3600), 'hour');
  return fmt.format(Math.round(diffSec / 86_400), 'day');
}
