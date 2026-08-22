// Mermaid diagram click-to-maximize (full viewport)
document.addEventListener('DOMContentLoaded', () => {
  // Create backdrop element
  const backdrop = document.createElement('div');
  backdrop.className = 'mermaid-backdrop';
  document.body.appendChild(backdrop);

  function maximizeDiagram(el) {
    el.classList.add('maximized');
    backdrop.classList.add('active');
    document.body.style.overflow = 'hidden';
  }

  function minimizeDiagram(el) {
    el.classList.remove('maximized');
    backdrop.classList.remove('active');
    document.body.style.overflow = '';
  }

  // Delegate clicks on Mermaid diagrams
  document.addEventListener('click', (e) => {
    const mermaidEl = e.target.closest('.mermaid');
    if (!mermaidEl) return;

    // If we click an already-maximized diagram, minimize it
    if (mermaidEl.classList.contains('maximized')) {
      minimizeDiagram(mermaidEl);
      return;
    }

    // Maximize
    maximizeDiagram(mermaidEl);
  });

  // Esc key to exit
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      const maximized = document.querySelector('.mermaid.maximized');
      if (maximized) minimizeDiagram(maximized);
    }
  });

  // Click backdrop to exit
  backdrop.addEventListener('click', () => {
    const maximized = document.querySelector('.mermaid.maximized');
    if (maximized) minimizeDiagram(maximized);
  });
});
