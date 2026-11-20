"""
Локальный DLP-детектор агента — для НЕМЕДЛЕННОЙ реакции (предупреждение
и блокировка) до отправки события на сервер.

Содержит подмножество критичных паттернов РК (карты, ИИН, IBAN, CVV,
грифы секретности). Полный набор политик остаётся на сервере.
"""
import re

# (имя правила, скомпилированный паттерн, blocking?)
_PATTERNS = [
    ("Номер банковской карты",
     re.compile(r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b"),
     True),
    ("CVV/CVC код карты",
     re.compile(r"\b(?:CVV|CVC|CVV2|CVC2|CID)[\s:=]?\d{3,4}\b", re.IGNORECASE),
     True),
    ("IBAN Казахстан",
     re.compile(r"\bKZ\d{2}[A-Z0-9]{3}\d{13}\b"),
     True),
    ("ИИН Казахстана",
     re.compile(r"\b\d{12}\b"),
     False),
    ("Гриф секретности",
     re.compile(r"(?:совершенно\s+секретно|коммерческая\s+тайна|для\s+служебного\s+пользования|строго\s+конфиденциально)", re.IGNORECASE),
     False),
]


def check(text: str) -> tuple[list[str], bool]:
    """
    Возвращает (список сработавших правил, требуется_ли_блокировка).
    """
    if not text:
        return [], False
    rules: list[str] = []
    block = False
    for name, pattern, is_block in _PATTERNS:
        if pattern.search(text):
            rules.append(name)
            if is_block:
                block = True
    return rules, block
