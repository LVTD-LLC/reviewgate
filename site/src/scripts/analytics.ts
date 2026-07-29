const posthogKey = import.meta.env.PUBLIC_POSTHOG_KEY;

if (posthogKey && navigator.doNotTrack !== "1") {
  const allowedOrigins = new Set([window.location.origin, "https://github.com"]);
  const storageKey = "reviewgate_anonymous_id";
  let distinctId: string = crypto.randomUUID();

  try {
    distinctId = localStorage.getItem(storageKey) ?? distinctId;
    localStorage.setItem(storageKey, distinctId);
  } catch {
    // Storage can be unavailable in strict privacy modes; the in-memory ID still works.
  }

  const capture = (event: string, properties: Record<string, string>) => {
    void fetch("https://us.i.posthog.com/i/v0/e/", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      keepalive: true,
      body: JSON.stringify({
        api_key: posthogKey,
        event,
        distinct_id: distinctId,
        properties: {
          ...properties,
          $process_person_profile: false,
        },
      }),
    }).catch(() => {
      // Analytics must never affect the site experience.
    });
  };

  const safeDestination = (value: string) => {
    try {
      const url = new URL(value, window.location.origin);
      return allowedOrigins.has(url.origin) ? `${url.origin}${url.pathname}` : "";
    } catch {
      return "";
    }
  };

  const referrerOrigin = () => {
    if (!document.referrer) {
      return "";
    }

    try {
      return new URL(document.referrer).origin;
    } catch {
      return "";
    }
  };

  capture("$pageview", {
    $current_url: `${window.location.origin}${window.location.pathname}`,
    $host: window.location.host,
    $pathname: window.location.pathname,
    $referrer: referrerOrigin(),
  });

  document.addEventListener("click", (event) => {
    if (!(event.target instanceof Element)) {
      return;
    }

    const link = event.target.closest<HTMLAnchorElement>("a[data-analytics-event]");
    const eventName = link?.dataset.analyticsEvent;

    if (!link || !eventName) {
      return;
    }

    capture(eventName, {
      destination: safeDestination(link.href),
      location: link.dataset.analyticsLocation ?? "unknown",
      page_path: window.location.pathname,
    });
  });
}
