import os
import re
import sys

def validate_skill(path):
    errors = []
    
    # Check SKILL.md
    skill_file = os.path.join(path, "SKILL.md")
    if not os.path.exists(skill_file):
        # Allow checking in current dir if it's a workflow
        if path.endswith(".md") and os.path.isfile(path):
            skill_file = path
        else:
            return [f"Missing SKILL.md in {path}"]

    with open(skill_file, "r", encoding="utf-8") as f:
        lines = f.readlines()
        
    content = "".join(lines)
    
    # 1. YAML Frontmatter check
    if not content.startswith("---"):
        errors.append(f"{skill_file}: Missing YAML frontmatter start")
    
    # 2. Line count check (< 500 lines)
    if len(lines) > 500:
        errors.append(f"{skill_file}: Too long ({len(lines)} lines, max 500)")
        
    # 3. Reference files check (only if directory)
    if os.path.isdir(path):
        ref_dir = os.path.join(path, "reference")
        if not os.path.exists(ref_dir):
            errors.append(f"{path}: Missing reference directory")
        else:
            playbook = os.path.join(ref_dir, "playbook.md")
            if not os.path.exists(playbook):
                errors.append(f"{path}: Missing reference/playbook.md")

    return errors

def main():
    target_dirs = [".agent/workflows"]
    all_errors = []
    
    for d in target_dirs:
        if not os.path.exists(d): continue
        if os.path.isfile(d):
            all_errors.extend(validate_skill(d))
        else:
            for item in os.listdir(d):
                item_path = os.path.join(d, item)
                all_errors.extend(validate_skill(item_path))
                
    if all_errors:
        print("Validation Failed:")
        for err in all_errors:
            print(f"  - {err}")
        sys.exit(1)
    else:
        print("All skills validated successfully!")

if __name__ == "__main__":
    main()
