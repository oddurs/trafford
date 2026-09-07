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

  setUpTheme();
  setUpCopy();
})();
