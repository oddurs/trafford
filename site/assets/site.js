/* Two enhancements, and nothing the page needs.
 *
 * Both controls are `hidden` in the markup and revealed here, so a reader with
 * JavaScript off sees no button that does nothing rather than a button that
 * silently fails. Navigation, content and the table of contents are HTML and
 * are never touched by this file. */

(function () {
  "use strict";

  var KEY = "trafford-theme";

  /* Theme names come from the generated stylesheet, which comes from the app's
     built-in themes — so the switcher offers exactly what `trafford` offers,
     and adding a theme to the app adds it here with no edit. */
  function themes() {
    var found = [];
    for (var i = 0; i < document.styleSheets.length; i++) {
      var rules;
      try {
        rules = document.styleSheets[i].cssRules;
      } catch (e) {
        continue; /* a stylesheet from another origin; there are none, but be safe */
      }
      for (var j = 0; j < rules.length; j++) {
        var m = /^\[data-theme="([^"]+)"\]$/.exec(rules[j].selectorText || "");
        if (m && found.indexOf(m[1]) === -1) found.push(m[1]);
      }
    }
    return found;
  }

  function setUpTheme() {
    var button = document.querySelector(".theme-toggle");
    if (!button) return;
    var names = themes();
    if (names.length < 2) return;

    var label = button.querySelector(".theme-name");
    var current = null;
    try {
      current = localStorage.getItem(KEY);
    } catch (e) {}

    function show(name) {
      if (label) label.textContent = name;
      button.setAttribute("aria-label", "Theme: " + name + ". Change it.");
    }

    show(current || names[0]);
    button.hidden = false;
    button.addEventListener("click", function () {
      var at = names.indexOf(current);
      current = names[(at + 1) % names.length];
      document.documentElement.setAttribute("data-theme", current);
      show(current);
      try {
        localStorage.setItem(KEY, current);
      } catch (e) {}
    });
  }

  function setUpCopy() {
    var buttons = document.querySelectorAll("[data-copy]");
    if (!navigator.clipboard) return;
    Array.prototype.forEach.call(buttons, function (button) {
      button.hidden = false;
      button.addEventListener("click", function () {
        navigator.clipboard.writeText(button.getAttribute("data-copy")).then(
          function () {
            var was = button.textContent;
            button.textContent = "Copied";
            setTimeout(function () {
              button.textContent = was;
            }, 1200);
          },
          function () {
            button.textContent = "Press ⌘C";
          }
        );
      });
    });
  }

  /* The cast player.
   *
   * A still cannot show what a terminal application is for, and asciinema
   * means a script from another host, which no page here loads. So the frames
   * are recorded by the same tool that takes the screenshots and replayed by
   * this — a cast is a list of rows of runs and nothing else.
   *
   * The poster that is already in the page is the content. A player replaces
   * it only once its frames have arrived, so with JavaScript off, with the
   * fetch failing, or before it lands, the page is complete.
   *
   * Nothing is fetched until a recording is near the viewport, and a player
   * stops when it leaves. Six of these all running behind the fold is a
   * laptop fan. */
  function Cast(host) {
    var poster = host.firstElementChild;
    var quiet = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    var button = document.createElement("button");
    button.type = "button";
    button.className = "cast-control";
    host.appendChild(button);

    var cast = null;
    var timer = null;
    var at = 0;
    var screen = null;
    var grid = null;
    var progress = null;
    var asked = false;

    function label(text) {
      button.textContent = text;
      button.setAttribute("aria-label", text + " the recording");
    }

    function build() {
      progress = document.createElement("div");
      progress.className = "cast-progress";
      host.appendChild(progress);
      screen = document.createElement("pre");
      screen.className = "cast-screen";
      screen.setAttribute("aria-hidden", "true");
      grid = [];
      for (var y = 0; y < cast.rows; y++) {
        var row = document.createElement("div");
        screen.appendChild(row);
        grid.push(row);
      }
      host.insertBefore(screen, poster);
      /* The screen is built but not shown. Loading a recording used to reveal
         its *first* frame, which is the least interesting one — the poster was
         chosen for being worth looking at. So the poster stays until the
         recording actually starts. */
      screen.style.visibility = "hidden";
      resize();
    }

    /* Scale the grid to the box rather than the box to the grid: the terminal
       is 120 columns and the box is whatever the reader's window makes it. */
    function resize() {
      if (!screen) return;
      var box = screen.getBoundingClientRect();
      if (!box.width) return;
      var size = Math.min(
        box.width / (cast.cols * 0.6),
        box.height / (cast.rows * 1.45)
      );
      screen.style.fontSize = Math.max(3, size).toFixed(2) + "px";
    }

    function paint(frame) {
      Object.keys(frame.rows).forEach(function (y) {
        var row = grid[y];
        if (!row) return;
        row.textContent = "";
        frame.rows[y].forEach(function (run) {
          var style = cast.styles[run[0]];
          var span = document.createElement("span");
          span.textContent = run[1];
          span.style.color = "#" + style[0];
          if (style[1] !== cast.bg) span.style.background = "#" + style[1];
          if (style[2]) span.style.fontWeight = "700";
          if (style[3]) span.style.fontStyle = "italic";
          if (style[4]) span.style.textDecoration = "underline";
          row.appendChild(span);
        });
      });
    }

    function step() {
      paint(cast.frames[at]);
      // Stepped rather than smooth: a recording of a terminal advances one
      // frame per keystroke, and a bar that slides pretends otherwise.
      if (progress) {
        progress.style.width = ((at + 1) / cast.frames.length) * 100 + "%";
      }
      var last = at === cast.frames.length - 1;
      var wait = last ? cast.loop : cast.frames[at].d;
      at = last ? 0 : at + 1;
      /* Every frame after the first carries only the rows that changed, so
         looping back to frame 0 has to clear the screen first or the last
         frame's rows stay behind. */
      if (at === 0) {
        timer = setTimeout(function () {
          grid.forEach(function (row) {
            row.textContent = "";
          });
          step();
        }, wait);
      } else {
        timer = setTimeout(step, wait);
      }
    }

    function play() {
      if (timer || !cast) return;
      label("Pause");
      /* `visibility`, not `display`: the poster stays in the flow and keeps
         reserving the box, so swapping them moves nothing. `hidden` would not
         have worked for the inline one anyway — it is an <svg>, and
         `HTMLElement.hidden` is not a property SVG elements have. */
      poster.style.visibility = "hidden";
      screen.style.visibility = "visible";
      if (progress) progress.style.opacity = "1";
      resize();
      step();
    }

    function pause() {
      clearTimeout(timer);
      timer = null;
      label("Play");
      if (progress) progress.style.opacity = "0";
    }

    function load() {
      if (asked) return Promise.resolve();
      asked = true;
      return fetch(host.getAttribute("data-cast"))
        .then(function (r) {
          if (!r.ok) throw new Error(r.status);
          return r.json();
        })
        .then(function (loaded) {
          cast = loaded;
          build();
          paint(cast.frames[0]);
          at = 1;
        })
        .catch(function () {
          /* The poster is still there and is still correct. */
          button.remove();
        });
    }

    button.addEventListener("click", function () {
      if (timer) pause();
      else if (cast) play();
      else load().then(play);
    });

    label("Play");
    return {
      near: function () {
        if (!quiet) load();
      },
      enter: function () {
        if (quiet) return;
        load().then(play);
      },
      leave: pause,
      resize: resize,
    };
  }

  function setUpCasts() {
    var hosts = document.querySelectorAll("[data-cast]");
    if (!hosts.length || !window.fetch) return;
    var players = Array.prototype.map.call(hosts, Cast);

    window.addEventListener("resize", function () {
      players.forEach(function (p) {
        p.resize();
      });
    });

    if (!window.IntersectionObserver) {
      players.forEach(function (p) {
        p.enter();
      });
      return;
    }

    /* Two observers: one that fetches a little before the recording arrives,
       and one that plays only while it is actually on screen. */
    var soon = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (e) {
          if (e.isIntersecting) players[hosts_index(e.target)].near();
        });
      },
      { rootMargin: "400px" }
    );
    var showing = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (e) {
          var player = players[hosts_index(e.target)];
          if (e.isIntersecting) player.enter();
          else player.leave();
        });
      },
      { threshold: 0.3 }
    );

    function hosts_index(el) {
      return Array.prototype.indexOf.call(hosts, el);
    }

    Array.prototype.forEach.call(hosts, function (host) {
      soon.observe(host);
      showing.observe(host);
    });
  }

  /* Click a theme in the strip and the page wears it.
   *
   * The strip is three recordings of one screen in three palettes, and the
   * palettes are the same ones the stylesheet carries — so making them
   * clickable costs almost nothing and is the only genuinely live thing on the
   * page. Without JavaScript they are three captioned pictures, which is what
   * they were. */
  function setUpThemeStrip() {
    var names = themes();
    if (names.length < 2) return;

    document.querySelectorAll("figure.shot").forEach(function (figure) {
      var caption = figure.querySelector("figcaption");
      if (!caption) return;
      var name = caption.textContent.trim().toLowerCase();
      if (names.indexOf(name) === -1) return;

      figure.classList.add("wearable");
      figure.tabIndex = 0;
      figure.setAttribute("role", "button");
      figure.setAttribute("aria-label", "Use the " + caption.textContent.trim() + " theme");

      function wear() {
        document.documentElement.setAttribute("data-theme", name);
        try {
          localStorage.setItem(KEY, name);
        } catch (e) {}
        var label = document.querySelector(".theme-toggle .theme-name");
        if (label) label.textContent = name;
      }

      figure.addEventListener("click", wear);
      figure.addEventListener("keydown", function (e) {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          wear();
        }
      });
    });
  }

  /* Sections arrive rather than being there.
   *
   * The hidden state is added by script, so with JavaScript off every section
   * is simply visible — a reveal that hides content when the script does not
   * run is a page that does not work. The hero is left alone: it is above the
   * fold and animating it would delay the only thing a reader came for. */
  function setUpReveal() {
    if (!window.IntersectionObserver) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

    var sections = document.querySelectorAll(".prose > .showcase");
    if (!sections.length) return;

    var seen = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (e) {
          if (!e.isIntersecting) return;
          e.target.classList.add("shown");
          seen.unobserve(e.target);
        });
      },
      { rootMargin: "0px 0px -12% 0px" }
    );

    sections.forEach(function (section) {
      section.classList.add("reveal");
      seen.observe(section);
    });
  }

  setUpTheme();
  setUpCopy();
  setUpCasts();
  setUpThemeStrip();
  setUpReveal();
})();
