print("=== CB Noten ===")
name = "Mia"
punkte = int(input())
if punkte >= 90:
    note = 1
elif punkte >= 75:
    note = 2
elif punkte >= 60:
    note = 3
elif punkte >= 50:
    note = 4
else:
    note = 5
print(f"Name: {name}")
print(f"Punkte: {punkte}")
print(f"Note: {note}")
