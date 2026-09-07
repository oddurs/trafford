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
   * this — about eighty lines, because a cast is a list of rows of runs and
   * nothing else.
   *
   * The still that is already in the figure is the content. This replaces it
   * only once the frames have arrived, so with JavaScript off, with the fetch
   * failing, or before it lands, the page is complete. */
  function setUpCast() {
    var figure = document.querySelector(".shot[data-cast]");
    if (!figure || !window.fetch) return;

    var still = figure.firstElementChild;
    var quiet = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    var button = document.createElement("button");
    button.type = "button";
    button.className = "cast-control";
    figure.appendChild(button);

    var cast = null;
    var timer = null;
    var at = 0;
    var screen = null;
    var grid = null;

    function label(text) {
      button.textContent = text;
      button.setAttribute("aria-label", text + " the recording");
    }

    function build() {
      screen = document.createElement("pre");
      screen.className = "cast-screen";
      screen.setAttribute("aria-hidden", "true");
      grid = [];
      for (var y = 0; y < cast.rows; y++) {
        var row = document.createElement("div");
        screen.appendChild(row);
        grid.push(row);
      }
      figure.insertBefore(screen, still);
      /* `visibility`, not `display`: the still stays in the flow and keeps
         reserving the box, so replacing it moves nothing. `hidden` would not
         have worked anyway — the still is an <svg>, and `HTMLElement.hidden`
         is not a property SVG elements have. */
      still.style.visibility = "hidden";
      resize();
    }

    /* Scale the grid to the box rather than the box to the grid: the terminal
       is 108 columns and the column it sits in is whatever the reader's window
       makes it. Below the floor it scrolls, like the still does. */
    function resize() {
      if (!screen) return;
      var box = screen.getBoundingClientRect();
      /* Fit both dimensions: the box is the still's, and a font size chosen
         from the width alone would overflow it on a short, wide column. */
      var size = Math.min(box.width / (cast.cols * 0.6), box.height / (cast.rows * 1.45));
      screen.style.fontSize = Math.max(4, size).toFixed(2) + "px";
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
      var last = at === cast.frames.length - 1;
      var wait = last ? cast.loop : cast.frames[at].d;
      at = last ? 0 : at + 1;
      /* Rebuilding from frame 0 after a loop: every frame after the first
         carries only the rows that changed, so jumping back has to repaint
         the whole screen or the last frame's rows stay behind. */
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
      if (timer) return;
      label("Pause");
      step();
    }

    function pause() {
      clearTimeout(timer);
      timer = null;
      label("Play");
    }

    button.addEventListener("click", function () {
      if (timer) pause();
      else play();
    });
    window.addEventListener("resize", resize);

    label(quiet ? "Play" : "Pause");
    fetch(figure.getAttribute("data-cast"))
      .then(function (r) {
        if (!r.ok) throw new Error(r.status);
        return r.json();
      })
      .then(function (loaded) {
        cast = loaded;
        build();
        /* Reduced motion gets the first frame and a control, never movement it
           did not ask for. */
        if (quiet) paint(cast.frames[0]);
        else play();
      })
      .catch(function () {
        /* The still is still there and is still correct. */
        button.remove();
      });
  }

  setUpTheme();
  setUpCopy();
  setUpCast();
})();
