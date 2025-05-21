export function setupContextMenu(containerEl, opts = {}) {
    const term = opts.term;
    const menu = document.getElementById("ctxMenu");

    let currentLayout = "qwerty";

    containerEl.addEventListener("contextmenu", (ev) => {
        if (ev.ctrlKey) {
            ev.preventDefault();
            showMenu(ev.clientX, ev.clientY);
        }
    });

    menu.addEventListener("click", (ev) => {
        const li = ev.target.closest(".menu-item");
        if (!li) return;

        const cmd = li.dataset.cmd;
        const layout = li.dataset.layout;
        hide();

        switch (cmd) {
            case "copy":
                if (term && term.hasSelection?.()) {
                    navigator.clipboard.writeText(term.getSelection());
                }
                break;
            case "paste":
                if (term) {
                    navigator.clipboard.readText().then((txt) => term.paste(txt));
                }
                break;
        }

        if (layout) {
            currentLayout = layout;
            updateLayoutUI();
            opts.onLayoutChange?.(layout);
        }
    });

    function showMenu(x, y) {
        menu.style.left = x + "px";
        menu.style.top = y + "px";
        menu.style.display = "block";

        const r = menu.getBoundingClientRect();

        addEventListener("click", closeOnce, { once: true });
        addEventListener("keydown", closeOnce, { once: true });
    }
    function hide() {
        menu.style.display = "none";
    }
    function closeOnce(e) {
        if (e.type === "keydown" && e.key !== "Escape") return;
        hide();
    }

    function updateLayoutUI() {
        menu.querySelectorAll("[data-layout]").forEach((li) =>
            li.classList.toggle("checked", li.dataset.layout === currentLayout),
        );
    }
    updateLayoutUI();
}
