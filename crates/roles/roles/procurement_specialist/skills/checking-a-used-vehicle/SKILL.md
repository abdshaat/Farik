---
name: checking-a-used-vehicle
description: "Use when the thing to buy is a used car: VIN, recalls, title, history report, inspection, comparable prices."
---

# Checking a used vehicle

A used car costs the most when something is wrong with it that nobody said. Check these in order,
and say what you could not check.

## 1. The VIN first

Find the vehicle identification number in the listing. If it is missing, say so and ask the founder
to get it from the seller: do not go on without it. Then:

If "Safety recalls" is connected, it covers vehicles sold in the United States:
- **Decode it** with `decode_vin`, and compare the year, make, model and engine it gives with the
  listing. If its `ErrorCode` is not 0, read its `ErrorText` and say the VIN did not decode cleanly.
- For the decoded make, model and year, read `vehicle_recalls` (open recalls, and any that say to
  stop driving the car or to park it outside), `vehicle_complaints` (how many, by component, and the
  newest, which are owners' words and not findings) and `vehicle_safety_ratings` (the crash-test
  stars of each version of the car).

Without it, or for a car sold elsewhere:
- **Decode it** on the national vehicle regulator's decoder, such as the United States' at
  `vpic.nhtsa.dot.gov`, and compare the year, make, model and engine it gives with the listing.
- **Look up its open recalls** on the regulator's recall lookup, such as `nhtsa.gov` in the United
  States, which takes the VIN.

Neither site is on Farik's list of sites you may read. Ask for what you need before you end your
turn, with `farik_request_sites`, one line of reason for each: "the official VIN decoder, to check
the listing" and "the official recall lookup by VIN". Your task then waits for the owner. If a site
is declined, say in your note which checks were not made.

## 2. The title

Say which state or country issued the title, and ask the seller what kind it is: clean, salvage,
rebuilt, or with a lien. A title that is not clean changes the price and may change whether the
business can resell the car.

## 3. The history report

Tell the founder to buy a vehicle history report for the VIN before money moves, and say what to
look for: accidents, odometer readings that fall, many owners, flood or fire damage, and unpaid
loans. You do not buy it.

## 4. An inspection

Tell the founder to have an independent mechanic inspect the car before money moves. Write what an
inspection should cover for this car's age and mileage.

## 5. Comparable prices

Find listings for the same year, make, model and mileage band, and write their asking prices, with
the address of each page and the day you read it. If "eBay listings" is connected, `search_items`
gives eBay's fixed-price asking prices, with `condition` `used` and a `min_price` and `max_price` to
keep to the band. Read eBay only through it, never by opening ebay.com pages; without it, say eBay
was not checked. A listing price is what a seller asks, not what a car sells for; say so.

## 6. What you write

You never say a car is sound. You report what the listing claims, what the checks found, what you
could not check, and the steps the founder should take before buying.
