"""
Маскирование персональных данных в отчётах и выгрузках.

Закон РК «О персональных данных» — принцип минимизации: в отчётах
чувствительные данные скрываются частично (видны последние символы
для сопоставления, остальное — звёздочки).

mask_text() применяется ко всем сниппетам при экспорте отчётов и при
показе инцидентов роли viewer.
"""
import re


def _mask_keep_last(match: re.Match, keep: int = 4) -> str:
    s = match.group(0)
    digits = re.sub(r"\D", "", s)
    if len(digits) <= keep:
        return "*" * len(s)
    return "*" * (len(s) - keep) + s[-keep:]


# Порядок важен: сначала более длинные/специфичные паттерны
_RULES = [
    # Номер банковской карты (13-19 цифр, возможно с пробелами/дефисами)
    (re.compile(r"\b(?:\d[ \-]?){13,19}\b"), lambda m: _mask_keep_last(m, 4)),
    # IBAN KZ
    (re.compile(r"\bKZ\d{2}[A-Z0-9]{16}\b"), lambda m: "KZ**" + "*" * 12 + m.group(0)[-4:]),
    # ИИН/БИН — 12 цифр
    (re.compile(r"\b\d{12}\b"), lambda m: "*" * 8 + m.group(0)[-4:]),
    # Паспорт РК
    (re.compile(r"\b[A-ZА-Я]{1,2}\d{7}\b"), lambda m: m.group(0)[:1] + "*" * (len(m.group(0)) - 3) + m.group(0)[-2:]),
    # CVV
    (re.compile(r"\b(?:CVV|CVC|CVV2|CVC2|CID)[\s:=]?\d{3,4}\b", re.IGNORECASE), lambda m: re.sub(r"\d", "*", m.group(0))),
    # Email — маскируем локальную часть
    (re.compile(r"\b([A-Za-z0-9._%+\-])[A-Za-z0-9._%+\-]*(@[A-Za-z0-9.\-]+\.[A-Za-z]{2,})\b"),
     lambda m: m.group(1) + "***" + m.group(2)),
    # Телефон РК/РФ
    (re.compile(r"(?<!\d)(?:\+7|8)[\s\-]?\(?\d{3}\)?[\s\-]?\d{3}[\s\-]?\d{2}[\s\-]?\d{2}(?!\d)"),
     lambda m: m.group(0)[:2] + "*" * (len(m.group(0)) - 4) + m.group(0)[-2:]),
]


def mask_text(text: str | None) -> str | None:
    """Маскирует все обнаруженные ПДн в строке."""
    if not text:
        return text
    result = text
    for pattern, repl in _RULES:
        result = pattern.sub(repl, result)
    return result
