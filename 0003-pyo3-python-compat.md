# PyO3 Python compat, not SIP reuse

Python's SIP/PyQt6 bindings are generated against C++ and die with it; we decided to serve identical `qgis.core` / `qgis.gui` paths from new PyO3 bindings verified by existing Python `test_*.py`, because "Python stays as-is" otherwise silently means "Python breaks".
