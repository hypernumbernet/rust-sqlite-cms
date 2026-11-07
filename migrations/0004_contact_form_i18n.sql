-- Localize built-in contact form widget UI via MiniJinja `ui` context.
UPDATE widget_types
SET html_template = '{% if form.sent %}
<div class="contact-form-message contact-form-success" role="status">{{ config.success_message }}</div>
{% else %}
<div class="contact-form-widget">
  {% if form.error %}
  <div class="contact-form-message contact-form-error" role="alert">{{ ui.submit_failed }}</div>
  {% endif %}
  {% if not form.token %}
  <div class="contact-form-message contact-form-error" role="alert">{{ ui.init_failed }}</div>
  {% else %}
  <h2 class="contact-form-heading">{{ config.heading }}</h2>
  <form class="contact-form" method="post" action="{{ form.action }}" novalidate>
    <input type="hidden" name="token" value="{{ form.token }}">
    <div class="form-group">
      <label for="contact-name-{{ form.id }}">{{ ui.name_label }} <span class="required" aria-hidden="true">*</span></label>
      <input type="text" id="contact-name-{{ form.id }}" name="name" required maxlength="100" autocomplete="name">
    </div>
    <div class="form-group">
      <label for="contact-email-{{ form.id }}">{{ ui.email_label }} <span class="required" aria-hidden="true">*</span></label>
      <input type="email" id="contact-email-{{ form.id }}" name="email" required maxlength="254" autocomplete="email">
    </div>
    {% if config.show_phone %}
    <div class="form-group">
      <label for="contact-phone-{{ form.id }}">{{ ui.phone_label }}</label>
      <input type="tel" id="contact-phone-{{ form.id }}" name="phone" maxlength="30" autocomplete="tel">
    </div>
    {% endif %}
    <div class="form-group">
      <label for="contact-message-{{ form.id }}">{{ ui.message_label }} <span class="required" aria-hidden="true">*</span></label>
      <textarea id="contact-message-{{ form.id }}" name="message" required maxlength="5000" rows="6"></textarea>
    </div>
    <button type="submit" class="button contact-form-submit">{{ config.submit_label }}</button>
  </form>
  {% endif %}
</div>
<style>
.contact-form-widget { max-width: 560px; }
.contact-form-heading { margin: 0 0 1.25rem; font-size: 1.5rem; }
.contact-form .form-group { margin-bottom: 1rem; }
.contact-form label { display: block; margin-bottom: 0.35rem; font-weight: 600; }
.contact-form .required { color: #b32d2e; }
.contact-form input[type="text"],
.contact-form input[type="email"],
.contact-form input[type="tel"],
.contact-form textarea {
  width: 100%; padding: 0.6rem 0.75rem; border: 1px solid #cbd5e1; border-radius: 6px; font: inherit;
}
.contact-form-submit { margin-top: 0.5rem; }
.contact-form-submit:disabled { opacity: 0.65; cursor: not-allowed; }
.contact-form-message { padding: 1rem 1.25rem; border-radius: 8px; margin: 1rem 0; }
.contact-form-success { background: #e8f5e9; border: 1px solid #81c784; color: #2e7d32; }
.contact-form-error { background: #fff5f5; border: 1px solid #f5c6cb; color: #b32d2e; }
</style>
<script>
(function() {
  var root = document.currentScript.previousElementSibling;
  if (!root || !root.classList.contains(''contact-form-widget'')) {
    root = document.currentScript.parentElement.querySelector(''.contact-form-widget'');
  }
  if (!root) return;
  var form = root.querySelector(''form.contact-form'');
  if (!form) return;
  var btn = form.querySelector(''.contact-form-submit'');
  var submittingText = "{{ ui.submitting }}";
  form.addEventListener(''submit'', function(e) {
    if (form.dataset.submitting === ''1'') {
      e.preventDefault();
      return;
    }
    form.dataset.submitting = ''1'';
    if (btn) {
      btn.disabled = true;
      btn.textContent = submittingText;
    }
  });
})();
</script>
{% endif %}',
    updated_at = datetime('now')
WHERE type_key = 'contact_form';
