def note(punkte):
    if punkte >= 90:
        return 1
    elif punkte >= 75:
        return 2
    elif punkte >= 60:
        return 3
    elif punkte >= 50:
        return 4
    else:
        return 5


print("=== CB Noten ===")
name = "Mia"
print(f"Name: {name}")
alle = [int(p) for p in input().split()]
for i, p in enumerate(alle, 1):
    print(f"Test {i}: {p} Punkte, Note {note(p)}")
print(f"Durchschnitt: {sum(alle) // len(alle)} Punkte")
